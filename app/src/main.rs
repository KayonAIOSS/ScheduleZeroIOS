//! ScheduleZero web app: Rust compiled to WebAssembly. Same features as the Android app, with a
//! phone layout tuned for iPhone and a separate desktop layout. See README.md.

mod actions;
mod app;
mod browser;
mod dom;
mod model;
mod push;
mod store;
mod sync;
mod time;
mod ui;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("ScheduleZero crashed: {info}").into());
    }));
    browser::glue("szRegisterServiceWorker", &[]);
    app::init();
    dom::install_listeners();
    dom::render();
    if let Some(boot) = browser::document().get_element_by_id("boot") {
        boot.remove();
    }
    sync::start();
    push::on_signed_in();
    wasm_bindgen_futures::spawn_local(sync::fetch_server_version());
}
