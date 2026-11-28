//! "Who's this?" and the password screen (create the first time, enter afterwards).

use super::*;
use crate::app::App;
use crate::model::Person;
use super::esc;

pub fn picker(desktop: bool) -> String {
    let mut people = String::new();
    for p in Person::ALL {
        people.push_str(&format!(
            r#"<button class="choice" data-act="pick" data-arg="{}"><span class="dot big" style="background:{}"></span><span class="t-title">{}</span></button>"#,
            p.id(),
            p.color(),
            p.name()
        ));
    }
    format!(
        r#"<div class="gate{}" data-key="gate-pick"><div class="gate-card"><div class="t-headline">Who's this?</div><div class="t-body dim mt8 center-text">Pick your dot. Your schedule syncs to every device you sign in on.</div><div class="choices mt32">{people}</div></div></div>"#,
        if desktop { " d" } else { "" }
    )
}

pub fn password(a: &App, me: Person, has_password: bool, desktop: bool) -> String {
    let setup = !has_password;
    let busy = a.ui.password_busy;
    let confirm = if setup {
        format!(
            r#"<label class="field mt10"><span class="field-label">Confirm password</span><span class="field-box"><input type="password" id="pw2" autocomplete="new-password" data-input="pw2" data-enter="pw-submit" value="{}"></span></label>"#,
            esc(&a.ui.password2)
        )
    } else {
        String::new()
    };
    let err = a.ui.password_error.as_ref().map(|e| format!(r#"<div class="t-body error mt10 center-text">{}</div>"#, esc(e))).unwrap_or_default();
    format!(
        r#"<div class="gate{}" data-key="gate-pw"><div class="gate-card">{}<div class="t-headline-s mt16 center-text">{}</div><div class="t-body dim mt8 center-text">{}</div>
<label class="field mt24"><span class="field-label">Password</span><span class="field-box"><input type="password" id="pw" autocomplete="{}" data-input="pw" data-enter="pw-submit" value="{}"></span></label>{confirm}{err}
<button class="btn filled full mt20" data-act="pw-submit"{}>{}</button>
<button class="btn text full mt8" data-act="pw-skip">Use offline for now</button>
<button class="btn text full" data-act="pw-switch">Not {}? Switch person</button></div></div>"#,
        if desktop { " d" } else { "" },
        format!(r#"<span class="dot" style="width:64px;height:64px;background:{}"></span>"#, me.color()),
        if setup { format!("Hi {}! Create a password", me.name()) } else { format!("Welcome back, {}", me.name()) },
        format!(
            "{}{}",
            if setup { "You'll use it to sign in on any of your devices." } else { "Enter your password to sync this device." },
            if a.status.auth_server.is_empty() { "" } else { " This site's server can't be reached right now, so this is for the backup server." }
        ),
        if setup { "new-password" } else { "current-password" },
        esc(&a.ui.password),
        if busy { " disabled" } else { "" },
        if busy { r#"<span class="spinner"></span>"# } else if setup { "Create password" } else { "Sign in" },
        me.name()
    )
}
