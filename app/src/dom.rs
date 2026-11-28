//! Rendering and events.
//!
//! Screens are plain functions that return HTML strings (see `ui`). Each redraw builds the new HTML,
//! then *morphs* the live page into it: only changed attributes and text are touched, so a focused
//! text field, the iPhone keyboard, scroll positions and running animations all survive redraws.
//! Clicks are handled with one listener on the document that looks for `data-act`.

use crate::app::{with, with_mut};
use crate::browser::{document, set_timeout, window};
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlInputElement, HtmlTemplateElement, Node};

thread_local! {
    static QUEUED: Cell<bool> = const { Cell::new(false) };
    static LAST_HTML: RefCell<String> = const { RefCell::new(String::new()) };
    static LAST_THEME: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn request_render() {
    if QUEUED.with(|q| q.replace(true)) {
        return;
    }
    let cb = Closure::once_into_js(|| {
        QUEUED.with(|q| q.set(false));
        render();
    });
    set_timeout(cb.unchecked_ref(), 0);
}

pub fn render() {
    let desktop = is_desktop();
    with_mut(|a| a.ui.desktop = desktop);
    let html = crate::ui::render_app();
    let same = LAST_HTML.with(|l| *l.borrow() == html);
    if !same {
        if let Some(root) = document().get_element_by_id("app") {
            let tpl: HtmlTemplateElement = document().create_element("template").unwrap().unchecked_into();
            tpl.set_inner_html(&html);
            morph_children(&root, &tpl.content());
        }
        LAST_HTML.with(|l| *l.borrow_mut() = html);
    }
    after_render();
}

fn after_render() {
    let (dark, overlay) = with(|a| (a.repo.data.settings.dark_mode, a.ui.editor.is_some() || a.ui.comparing.is_some() || a.ui.confirm.is_some() || a.ui.carpool.is_some()));
    let theme = if dark { "dark" } else { "light" }.to_string();
    let changed = LAST_THEME.with(|t| *t.borrow() != theme);
    if let Some(html) = document().document_element() {
        if changed {
            let _ = html.set_attribute("data-theme", &theme);
            if let Ok(Some(meta)) = document().query_selector("meta[name=theme-color]") {
                let _ = meta.set_attribute("content", if dark { "#000000" } else { "#F6FBFD" });
            }
            LAST_THEME.with(|t| *t.borrow_mut() = theme);
        }
        let _ = if overlay { html.class_list().add_1("lock") } else { html.class_list().remove_1("lock") };
    }
    // A time list that just opened scrolls its selected slot into view, once.
    if let Ok(Some(sel)) = document().query_selector("[data-scroll-once]") {
        sel.remove_attribute("data-scroll-once").ok();
        let opts = web_sys::ScrollIntoViewOptions::new();
        opts.set_block(web_sys::ScrollLogicalPosition::Center);
        sel.scroll_into_view_with_scroll_into_view_options(&opts);
    }
}

/// "#inbox" in the address (from a notification tap) opens the inbox.
pub fn open_from_hash() {
    let hash = window().location().hash().unwrap_or_default();
    if hash == "#inbox" {
        let desktop = is_desktop();
        crate::actions::click(if desktop { "side" } else { "nav" }, "inbox");
        if let Ok(h) = window().history() {
            let _ = h.replace_state_with_url(&JsValue::NULL, "", Some("#"));
        }
    }
}

pub fn is_desktop() -> bool {
    window().match_media("(min-width: 900px)").ok().flatten().map(|m| m.matches()).unwrap_or(false)
}

// ---- morphing -------------------------------------------------------------------------------

fn children(n: &Node) -> Vec<Node> {
    let list = n.child_nodes();
    (0..list.length()).filter_map(|i| list.item(i)).collect()
}

fn same_kind(a: &Node, b: &Node) -> bool {
    if a.node_type() != b.node_type() {
        return false;
    }
    match (a.dyn_ref::<Element>(), b.dyn_ref::<Element>()) {
        (Some(x), Some(y)) => x.tag_name() == y.tag_name() && x.get_attribute("data-key") == y.get_attribute("data-key"),
        _ => true,
    }
}

fn morph_children(old: &Node, new: &Node) {
    let olds = children(old);
    let news = children(new);
    for (i, n) in news.iter().enumerate() {
        match olds.get(i) {
            Some(o) if same_kind(o, n) => patch(o, n),
            Some(o) => {
                let _ = old.replace_child(n, o);
            }
            None => {
                let _ = old.append_child(n);
            }
        }
    }
    for o in olds.iter().skip(news.len()) {
        let _ = old.remove_child(o);
    }
}

fn patch(old: &Node, new: &Node) {
    let (Some(oe), Some(ne)) = (old.dyn_ref::<Element>(), new.dyn_ref::<Element>()) else {
        if old.node_value() != new.node_value() {
            old.set_node_value(new.node_value().as_deref());
        }
        return;
    };
    let attrs = ne.attributes();
    for i in 0..attrs.length() {
        if let Some(a) = attrs.item(i) {
            let (name, value) = (a.name(), a.value());
            if oe.get_attribute(&name).as_deref() != Some(value.as_str()) {
                let _ = oe.set_attribute(&name, &value);
            }
        }
    }
    let old_attrs = oe.attributes();
    let stale: Vec<String> = (0..old_attrs.length())
        .filter_map(|i| old_attrs.item(i))
        .map(|a| a.name())
        .filter(|n| !ne.has_attribute(n))
        .collect();
    for n in stale {
        let _ = oe.remove_attribute(&n);
    }
    if let Some(input) = oe.dyn_ref::<HtmlInputElement>() {
        let focused = document().active_element().map(|f| f == *oe).unwrap_or(false);
        let want = ne.get_attribute("value").unwrap_or_default();
        if !focused && input.value() != want {
            input.set_value(&want);
        }
        input.set_checked(ne.has_attribute("checked"));
    }
    morph_children(old, new);
}

// ---- events ---------------------------------------------------------------------------------

fn closest_with(target: Option<web_sys::EventTarget>, attr: &str) -> Option<Element> {
    let el: Element = target?.dyn_into::<Node>().ok().and_then(|n| {
        if n.node_type() == Node::ELEMENT_NODE {
            n.dyn_into::<Element>().ok()
        } else {
            n.parent_element()
        }
    })?;
    el.closest(&format!("[{attr}]")).ok().flatten()
}

fn listen(target: &web_sys::EventTarget, event: &str, f: impl FnMut(web_sys::Event) + 'static) {
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref());
    cb.forget();
}

pub fn install_listeners() {
    let doc: web_sys::EventTarget = document().into();
    listen(&doc, "click", |e| {
        if let Some(el) = closest_with(e.target(), "data-act") {
            if el.has_attribute("disabled") {
                return;
            }
            let act = el.get_attribute("data-act").unwrap_or_default();
            let arg = el.get_attribute("data-arg").unwrap_or_default();
            e.prevent_default();
            crate::actions::click(&act, &arg);
            request_render();
        }
    });
    listen(&doc, "input", |e| {
        if let Some(el) = closest_with(e.target(), "data-input") {
            let name = el.get_attribute("data-input").unwrap_or_default();
            let value = el.dyn_ref::<HtmlInputElement>().map(|i| i.value()).unwrap_or_default();
            if crate::actions::input(&name, &value) {
                request_render();
            }
        }
    });
    listen(&doc, "change", |e| {
        if let Some(el) = closest_with(e.target(), "data-change") {
            let name = el.get_attribute("data-change").unwrap_or_default();
            let input = el.dyn_ref::<HtmlInputElement>();
            let value = input.map(|i| i.value()).unwrap_or_default();
            let checked = input.map(|i| i.checked()).unwrap_or(false);
            crate::actions::change(&name, &value, checked);
            request_render();
        }
    });
    listen(&doc, "keydown", |e| {
        let Some(k) = e.dyn_ref::<web_sys::KeyboardEvent>() else { return };
        if k.key() == "Enter" {
            if let Some(el) = closest_with(e.target(), "data-enter") {
                let act = el.get_attribute("data-enter").unwrap_or_default();
                e.prevent_default();
                crate::actions::click(&act, "");
                request_render();
            }
        } else if k.key() == "Escape" {
            crate::actions::click("close-overlay", "");
            request_render();
        }
    });
    let win: web_sys::EventTarget = window().into();
    listen(&win, "popstate", |_| {
        crate::actions::history_back();
        request_render();
    });
    listen(&win, "resize", |_| {
        let d = is_desktop();
        if with(|a| a.ui.desktop) != d {
            request_render();
        }
    });
    listen(&win, "online", |_| crate::sync::wake_up());
    // A tapped notification asks the open page to show the inbox (see glue.js / sw.js).
    listen(&win, "hashchange", |_| {
        open_from_hash();
        request_render();
    });
    listen(&doc, "visibilitychange", |_| {
        if document().visibility_state() == web_sys::VisibilityState::Visible {
            crate::sync::start();
            crate::push::on_signed_in();
            request_render();
        } else {
            crate::sync::stop();
        }
    });
    // Countdowns stay live, like Android's rememberNow().
    let tick = Closure::<dyn FnMut()>::new(request_render);
    let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 5_000);
    tick.forget();
}
