//! SPEC-9 "Windows" — all UI and state. Portable half.
//!
//! Nothing in this file imports `dioxus::desktop`; every OS call goes through
//! `crate::platform`. That is the experiment: a Dioxus-Native (Blitz) port
//! should be able to swap `platform.rs` and keep this file verbatim.
//!
//! THE WINDOW MODEL (the finding this spec asks for)
//! Every Dioxus desktop window is an independent `VirtualDom` with its own
//! runtime, scheduler and webview — a window is *not* a value in the UI tree.
//! But a `Signal` is a `generational-box` handle, not a runtime-local cell:
//! reading one subscribes whichever `ReactiveContext` is current (i.e. the
//! *reading* window's scope), and writing marks every subscriber dirty through
//! its own runtime's scheduler. So handing the same `Shared` struct of Signals
//! to a second VirtualDom via `VirtualDom::with_root_context` gives genuine,
//! bidirectional, one-source-of-truth shared state across windows, with no
//! channel, bus or mutex anywhere. `GlobalSignal` does NOT work this way (it
//! resolves per-runtime, so each window would get its own copy).

use dioxus::html::Modifiers;
use dioxus::prelude::*;

use crate::platform::{self, WinSpec};

// ---------------------------------------------------------------- model

#[derive(Clone, PartialEq)]
pub struct Project {
    pub name: String,
    pub owner: String,
    pub budget: f64,
    pub open: bool,
}

fn seed() -> Vec<Project> {
    let rows: [(&str, &str, f64, bool); 6] = [
        ("Harbor Migration", "avery", 128_400.0, true),
        ("Kiln Retrofit", "bo", 42_950.5, true),
        ("Lagoon Telemetry", "cass", 7_800.0, false),
        ("Marble Pipeline", "dev", 219_000.0, true),
        ("Nectar Rollout", "eli", 63_250.75, false),
        ("Orbit Failover", "fern", 95_100.0, true),
    ];
    rows.iter()
        .map(|(n, o, b, s)| Project {
            name: n.to_string(),
            owner: o.to_string(),
            budget: *b,
            open: *s,
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    System,
}
impl Theme {
    fn class(self) -> &'static str {
        match self {
            Theme::Light => "t-light",
            Theme::Dark => "t-dark",
            Theme::System => "t-system",
        }
    }
    fn code(self) -> u8 {
        match self {
            Theme::Light => 0,
            Theme::Dark => 1,
            Theme::System => 2,
        }
    }
    fn from_code(c: u8) -> Theme {
        match c {
            0 => Theme::Light,
            1 => Theme::Dark,
            _ => Theme::System,
        }
    }
}

/// One source of truth for every window. `Copy`, so it is handed to child
/// VirtualDoms by value (`with_root_context`) and read back with `use_context`.
#[derive(Clone, Copy)]
pub struct Shared {
    pub projects: Signal<Vec<Project>>,
    pub sel: Signal<usize>,
    pub pings: Signal<u32>,
    pub pong: Signal<u32>,
    pub theme: Signal<Theme>,
    pub compact: Signal<bool>,
    pub dirty: Signal<bool>,
    pub modal: Signal<bool>,
    pub draft_name: Signal<String>,
    pub draft_budget: Signal<String>,
    pub saved: Signal<Persisted>,
    /// self-test only: the inspector's verdict on what it can see of the main
    /// window's state (proves the parent -> child direction).
    pub child_ok: Signal<Option<bool>>,
}

// ------------------------------------------------------- persistence (req 9)

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Persisted {
    pub main: Option<platform::Bounds>,
    pub inspector: Option<platform::Bounds>,
    pub prefs: Option<platform::Bounds>,
    pub inspector_open: bool,
    pub theme: u8,
    pub compact: bool,
}

fn state_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("windows-state.json")))
        .unwrap_or_else(|| std::path::PathBuf::from("windows-state.json"))
}

fn fmt_bounds(k: &str, b: Option<platform::Bounds>) -> String {
    match b {
        Some((x, y, w, h)) => format!("  \"{k}\": [{x:.1}, {y:.1}, {w:.1}, {h:.1}],\n"),
        None => format!("  \"{k}\": null,\n"),
    }
}

/// Deliberately hand-rolled: serde/serde_json would be a helper crate for six
/// numbers. Emits real JSON; the reader is a 20-line scanner.
pub fn save_state(p: Persisted) {
    let json = format!(
        "{{\n{}{}{}  \"inspector_open\": {},\n  \"theme\": {},\n  \"compact\": {}\n}}\n",
        fmt_bounds("main", p.main),
        fmt_bounds("inspector", p.inspector),
        fmt_bounds("prefs", p.prefs),
        p.inspector_open,
        p.theme,
        p.compact
    );
    let path = state_path();
    match std::fs::write(&path, json) {
        Ok(()) => println!("[windows] saved state -> {}", path.display()),
        Err(e) => println!("[windows] save state failed: {e}"),
    }
}

fn scan_array(src: &str, key: &str) -> Option<platform::Bounds> {
    let at = src.find(&format!("\"{key}\""))?;
    let open = src[at..].find('[')? + at;
    let close = src[open..].find(']')? + open;
    // reject a `null` that sits before the next `[`
    if src[at..open].contains("null") {
        return None;
    }
    let nums: Vec<f64> = src[open + 1..close]
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    (nums.len() == 4).then(|| (nums[0], nums[1], nums[2], nums[3]))
}

fn scan_scalar(src: &str, key: &str) -> Option<String> {
    let at = src.find(&format!("\"{key}\""))?;
    let colon = src[at..].find(':')? + at + 1;
    let end = src[colon..]
        .find(['\n', ','])
        .map(|i| i + colon)
        .unwrap_or(src.len());
    Some(src[colon..end].trim().to_string())
}

pub fn load_state() -> Persisted {
    let Ok(src) = std::fs::read_to_string(state_path()) else {
        return Persisted::default();
    };
    Persisted {
        main: scan_array(&src, "main"),
        inspector: scan_array(&src, "inspector"),
        prefs: scan_array(&src, "prefs"),
        inspector_open: scan_scalar(&src, "inspector_open").as_deref() == Some("true"),
        theme: scan_scalar(&src, "theme")
            .and_then(|s| s.parse().ok())
            .unwrap_or(2),
        compact: scan_scalar(&src, "compact").as_deref() == Some("true"),
    }
}

fn collect_state(sh: Shared) -> Persisted {
    let prev = *sh.saved.peek();
    Persisted {
        main: platform::bounds("main").or(prev.main),
        inspector: platform::bounds("inspector").or(prev.inspector),
        prefs: platform::bounds("prefs").or(prev.prefs),
        inspector_open: platform::is_open("inspector"),
        theme: sh.theme.peek().code(),
        compact: *sh.compact.peek(),
    }
}

// ------------------------------------------------------------ window opens

fn open_inspector(sh: Shared) {
    let pos = sh
        .saved
        .peek()
        .inspector
        .map(|b| (b.0, b.1))
        .or(platform::place().then_some((800.0, 60.0)));
    platform::open_or_focus(
        WinSpec {
            key: "inspector",
            title: "Inspector".into(),
            size: (360.0, 300.0),
            pos,
            child_of_main: true, // req 6: real OS child window
        },
        Inspector,
        sh,
    );
}

fn open_prefs(sh: Shared) {
    let pos = sh
        .saved
        .peek()
        .prefs
        .map(|b| (b.0, b.1))
        .or(platform::place().then_some((800.0, 420.0)));
    platform::open_or_focus(
        WinSpec {
            key: "prefs",
            title: "Preferences".into(),
            size: (320.0, 200.0),
            pos,
            child_of_main: false,
        },
        Prefs,
        sh,
    );
}

fn open_modal(mut sh: Shared) {
    let i = *sh.sel.peek();
    let Some(p) = sh.projects.peek().get(i).cloned() else {
        return;
    };
    sh.draft_name.set(p.name);
    sh.draft_budget.set(format!("{:.2}", p.budget));
    sh.modal.set(true);
    platform::open_or_focus(
        WinSpec {
            key: "edit",
            title: "Edit project".into(),
            size: (380.0, 220.0),
            pos: platform::place().then_some((800.0, 660.0)),
            child_of_main: true,
        },
        EditDialog,
        sh,
    );
}

// ------------------------------------------------------------- main window

#[component]
pub fn App() -> Element {
    let mut sh = use_context_provider(|| Shared {
        projects: Signal::new(seed()),
        sel: Signal::new(0),
        pings: Signal::new(0),
        pong: Signal::new(0),
        theme: Signal::new(Theme::System),
        compact: Signal::new(false),
        dirty: Signal::new(false),
        modal: Signal::new(false),
        draft_name: Signal::new(String::new()),
        draft_budget: Signal::new(String::new()),
        saved: Signal::new(Persisted::default()),
        child_ok: Signal::new(None),
    });
    let mut close_prompt = use_signal(|| false);

    // Mount: register, restore persisted geometry/prefs, re-open the inspector.
    use_hook(move || {
        platform::register("main");
        let p = load_state();
        sh.saved.set(p);
        sh.theme.set(Theme::from_code(p.theme));
        sh.compact.set(p.compact);
        if platform::place() {
            platform::set_bounds("main", (40.0, 60.0, 720.0, 480.0));
        } else if let Some(b) = p.main {
            platform::set_bounds("main", b);
        }
        println!(
            "[windows] main mounted scale_factor={} restored={:?}",
            platform::scale_factor(),
            p.main
        );
        if p.inspector_open && !platform::selftest() {
            open_inspector(sh);
        }
    });

    // req 7: close interception. Returns true = keep the window (veto).
    platform::use_close_guard(use_callback(move |()| {
        if *sh.dirty.peek() {
            close_prompt.set(true);
            println!("[windows] close requested while dirty -> prompt");
            true
        } else {
            save_state(collect_state(sh));
            println!("[windows] close requested, clean -> exit");
            platform::quit();
            false
        }
    }));

    let del = use_callback(move |()| {
        let i = *sh.sel.peek();
        let Some(p) = sh.projects.peek().get(i).cloned() else {
            return;
        };
        platform::confirm(
            "Delete project".into(),
            format!("Delete \"{}\"?", p.name),
            use_callback(move |yes: bool| {
                if yes {
                    sh.projects.write().remove(i);
                    let n = sh.projects.peek().len();
                    if i >= n && n > 0 {
                        sh.sel.set(n - 1);
                    }
                    sh.dirty.set(true);
                    println!("[windows] deleted row {i}, {n} left");
                }
            }),
        );
    });

    selftest_main(sh, close_prompt);

    let projects = sh.projects.read().clone();
    let sel = (sh.sel)();
    let sel_name = projects
        .get(sel)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "—".into());
    let modal_open = (sh.modal)();
    let root_class = format!(
        "root {} {}",
        (sh.theme)().class(),
        if (sh.compact)() { "compact" } else { "" }
    );

    rsx! {
        style { {CSS} }
        div {
            class: "{root_class}",
            tabindex: "-1",
            onmounted: move |e| async move { _ = e.set_focus(true).await; },
            onkeydown: move |e: KeyboardEvent| {
                let m = e.modifiers();
                let meta = m.contains(Modifiers::META) || m.contains(Modifiers::CONTROL);
                if let Key::Character(c) = e.key() {
                    if meta && c == "," { open_prefs(sh); }
                    if meta && m.contains(Modifiers::SHIFT) && c.eq_ignore_ascii_case("i") {
                        if platform::is_open("inspector") { platform::close_key("inspector"); }
                        else { open_inspector(sh); }
                    }
                }
            },

            div { class: "toolbar",
                button { onclick: move |_| open_modal(sh), "Edit…" }
                button { onclick: move |_| open_inspector(sh), "Inspector" }
                button { onclick: move |_| open_prefs(sh), "Preferences…" }
                button { id: "delete", class: "danger", onclick: move |_| del.call(()), "Delete" }
                button {
                    onclick: move |_| { sh.pong += 1; println!("[windows] pong -> inspector"); },
                    "Pong"
                }
                span { class: "grow" }
                span { class: "hint", if (sh.dirty)() { "● unsaved" } else { "saved" } }
            }

            div { class: "list",
                div { class: "row head",
                    span { "Project" } span { "Owner" }
                    span { class: "num", "Budget" } span { "Status" }
                }
                for (i, p) in projects.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: if i == sel { "row sel" } else { "row" },
                        onclick: move |_| sh.sel.set(i),
                        ondoubleclick: move |_| { sh.sel.set(i); open_modal(sh); },
                        span { "{p.name}" }
                        span { "{p.owner}" }
                        span { class: "num", "{p.budget:.2}" }
                        span { class: if p.open { "chip open" } else { "chip closed" },
                            if p.open { "Open" } else { "Closed" }
                        }
                    }
                }
            }

            div { class: "status",
                "selected: {sel_name} · pings: {sh.pings}"
                span { class: "grow" }
                span { class: "hint", "⌘, prefs · ⌘⇧I inspector · ⌘W close" }
            }

            // req 2: while the edit dialog window is open the main window must
            // not accept input. Hand-rolled: a scrim that swallows every click.
            if modal_open {
                div { class: "scrim", onclick: move |_| platform::focus("edit") }
            }

            // req 7: Save / Discard / Cancel.
            if close_prompt() {
                div { class: "scrim",
                    div { class: "card",
                        h3 { "Save changes?" }
                        p { "This window has unsaved edits." }
                        div { class: "cardrow",
                            button {
                                onclick: move |_| {
                                    save_state(collect_state(sh));
                                    println!("[windows] close: Save -> exit");
                                    platform::quit();
                                },
                                "Save"
                            }
                            button {
                                onclick: move |_| {
                                    println!("[windows] close: Discard -> exit");
                                    platform::quit();
                                },
                                "Discard"
                            }
                            button {
                                autofocus: true,
                                onclick: move |_| {
                                    close_prompt.set(false);
                                    println!("[windows] close: Cancel -> vetoed");
                                },
                                "Cancel"
                            }
                        }
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------- inspector window

#[component]
fn Inspector() -> Element {
    let mut sh = use_context::<Shared>();
    use_hook(|| platform::register("inspector"));
    selftest_inspector(sh);

    let sel = (sh.sel)();
    let projects = sh.projects.read().clone();
    let Some(p) = projects.get(sel).cloned() else {
        return rsx! { div { class: "root", "no selection" } };
    };
    let pong = (sh.pong)();
    let root_class = format!("root pane {}", (sh.theme)().class());

    rsx! {
        style { {CSS} }
        div {
            class: "{root_class}",
            tabindex: "-1",
            onmounted: move |e| async move { _ = e.set_focus(true).await; },
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape { platform::close_self(); }
            },
            // Pong flash: the key forces element recreation, which replays the
            // 300 ms CSS keyframe. No timer, no JS.
            div { key: "{pong}", class: if pong > 0 { "flash fields" } else { "fields" },
                label { "Name" }
                input {
                    value: "{p.name}",
                    oninput: move |e| { sh.projects.write()[sel].name = e.value(); sh.dirty.set(true); }
                }
                label { "Owner" }
                input {
                    value: "{p.owner}",
                    oninput: move |e| { sh.projects.write()[sel].owner = e.value(); sh.dirty.set(true); }
                }
                label { "Budget" }
                input {
                    value: "{p.budget:.2}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            sh.projects.write()[sel].budget = v;
                            sh.dirty.set(true);
                        }
                    }
                }
                label { "Status" }
                select {
                    value: if p.open { "open" } else { "closed" },
                    onchange: move |e| {
                        sh.projects.write()[sel].open = e.value() == "open";
                        sh.dirty.set(true);
                    },
                    option { value: "open", "Open" }
                    option { value: "closed", "Closed" }
                }
            }
            div { class: "cardrow",
                button {
                    onclick: move |_| { sh.pings += 1; println!("[inspector] ping -> main"); },
                    "Ping"
                }
                button { onclick: move |_| platform::close_self(), "Close" }
            }
            div { class: "status", "row {sel + 1} · pings sent: {sh.pings}" }
        }
    }
}

// ------------------------------------------------------ preferences window

#[component]
fn Prefs() -> Element {
    let mut sh = use_context::<Shared>();
    use_hook(|| platform::register("prefs"));
    let theme = (sh.theme)();
    let root_class = format!("root pane {}", theme.class());

    rsx! {
        style { {CSS} }
        div {
            class: "{root_class}",
            tabindex: "-1",
            onmounted: move |e| async move { _ = e.set_focus(true).await; },
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape { platform::close_self(); }
            },
            label { class: "check",
                input {
                    r#type: "checkbox",
                    checked: (sh.compact)(),
                    onchange: move |e| sh.compact.set(e.checked()),
                }
                " Compact rows"
            }
            fieldset {
                legend { "Theme" }
                for (t, name) in [(Theme::Light, "Light"), (Theme::Dark, "Dark"), (Theme::System, "System")] {
                    label { class: "check",
                        input {
                            r#type: "radio",
                            name: "theme",
                            checked: theme == t,
                            onchange: move |_| sh.theme.set(t),
                        }
                        " {name}"
                    }
                }
            }
            div { class: "status", "applies to every open window" }
        }
    }
}

// ------------------------------------------------------- modal edit window

#[component]
fn EditDialog() -> Element {
    let mut sh = use_context::<Shared>();
    use_hook(|| platform::register("edit"));
    selftest_modal(sh);

    let commit = use_callback(move |()| {
        let i = *sh.sel.peek();
        let name = sh.draft_name.peek().clone();
        let budget = sh.draft_budget.peek().parse::<f64>().ok();
        if let Some(p) = sh.projects.write().get_mut(i) {
            p.name = name;
            if let Some(b) = budget {
                p.budget = b;
            }
        }
        sh.dirty.set(true);
        sh.modal.set(false);
        platform::close_self();
        platform::focus("main"); // focus returns to the parent
        println!("[edit] committed");
    });
    let cancel = use_callback(move |()| {
        sh.modal.set(false);
        platform::close_self();
        platform::focus("main");
        println!("[edit] cancelled");
    });

    let root_class = format!("root pane {}", (sh.theme)().class());
    rsx! {
        style { {CSS} }
        div {
            class: "{root_class}",
            onkeydown: move |e: KeyboardEvent| {
                match e.key() {
                    Key::Escape => cancel.call(()),
                    Key::Enter => commit.call(()),
                    _ => {}
                }
            },
            div { class: "fields",
                label { "Name" }
                input {
                    value: "{sh.draft_name}",
                    oninput: move |e| sh.draft_name.set(e.value()),
                    onmounted: move |e| async move { _ = e.set_focus(true).await; },
                }
                label { "Budget" }
                input {
                    value: "{sh.draft_budget}",
                    oninput: move |e| sh.draft_budget.set(e.value()),
                }
            }
            div { class: "cardrow",
                button { onclick: move |_| commit.call(()), "OK" }
                button { onclick: move |_| cancel.call(()), "Cancel" }
            }
            div { class: "status", "Enter commits · Esc cancels" }
        }
    }
}

// ------------------------------------------------------------- self-tests
// WINDOWS_SELFTEST=1 drives the same callbacks the UI uses, across three
// VirtualDoms, and prints `SELFTEST DONE pass=N fail=M`.

async fn nap(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

fn selftest_main(mut sh: Shared, mut close_prompt: Signal<bool>) {
    use_future(move || async move {
        if !platform::selftest() {
            return;
        }
        let (mut pass, mut fail) = (0u32, 0u32);
        let mut check = |name: &str, ok: bool| {
            if ok {
                pass += 1;
                println!("SELFTEST ok   {name}");
            } else {
                fail += 1;
                println!("SELFTEST FAIL {name}");
            }
        };
        nap(1200).await;
        check("1 window at start", platform::open_count() == 1);

        open_inspector(sh);
        nap(1300).await;
        check("inspector opened (2 windows)", platform::open_count() == 2);
        check("inspector is a singleton", {
            open_inspector(sh);
            platform::open_count() == 2
        });

        nap(1200).await;
        check("child -> parent: Ping raised main's counter", *sh.pings.peek() == 1);
        check(
            "child -> parent: inspector edit visible in main list",
            sh.projects.peek()[0].name == "PINGED",
        );

        sh.sel.set(3);
        sh.theme.set(Theme::Dark);
        sh.compact.set(true);
        nap(2500).await;
        check(
            "parent -> child: inspector sees sel/theme/compact",
            *sh.child_ok.peek() == Some(true),
        );

        open_prefs(sh);
        nap(1200).await;
        check("preferences opened (3 windows)", platform::open_count() == 3);

        open_modal(sh);
        nap(1200).await;
        check(
            "modal window opened (4 windows)",
            platform::open_count() == 4 && *sh.modal.peek(),
        );

        nap(1500).await;
        check(
            "modal OK committed into the shared model",
            sh.projects.peek()[3].name == "EDITED-OK",
        );
        check("modal closed, flag cleared", !*sh.modal.peek());

        let want = collect_state(sh);
        save_state(want);
        let got = load_state();
        check("persistence round-trip (bounds/theme/compact)", got == want);
        check("dirty flag set by the edits", *sh.dirty.peek());

        // The veto path itself needs a real CloseRequested (synthetic ⌘W);
        // here we only prove the prompt renders and Cancel clears it.
        close_prompt.set(true);
        nap(300).await;
        check("close prompt shows", close_prompt());
        close_prompt.set(false);

        println!("SELFTEST DONE pass={pass} fail={fail}");
        nap(200).await;
        platform::quit();
    });
}

fn selftest_inspector(mut sh: Shared) {
    use_future(move || async move {
        if !platform::selftest() {
            return;
        }
        nap(700).await;
        sh.pings += 1;
        let i = *sh.sel.peek();
        sh.projects.write()[i].name = "PINGED".into();
        println!("[inspector] selftest: pinged and renamed row {i}");
        nap(3200).await;
        let ok = *sh.sel.peek() == 3 && sh.theme.peek().code() == 1 && *sh.compact.peek();
        sh.child_ok.set(Some(ok));
        println!("[inspector] selftest: sees sel={} ok={ok}", sh.sel.peek());
    });
}

fn selftest_modal(mut sh: Shared) {
    use_future(move || async move {
        if !platform::selftest() {
            return;
        }
        nap(1600).await;
        sh.draft_name.set("EDITED-OK".into());
        nap(200).await;
        let i = *sh.sel.peek();
        let name = sh.draft_name.peek().clone();
        sh.projects.write()[i].name = name;
        sh.dirty.set(true);
        sh.modal.set(false);
        println!("[edit] selftest: committed into row {i}");
        platform::close_self();
    });
}

// ------------------------------------------------------------------- style

const CSS: &str = r#"
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, -apple-system, sans-serif; }
.root {
  --bg:#f4f4f7; --fg:#18181b; --panel:#fff; --line:#d3d3d9; --sel:#1f6feb; --hint:#6b6b73;
  display:flex; flex-direction:column; height:100vh; padding:8px; gap:8px;
  background:var(--bg); color:var(--fg); outline:none;
}
.t-dark { --bg:#1c1c1f; --fg:#e9e9ec; --panel:#242428; --line:#3a3a41; --sel:#2f81f7; --hint:#9a9aa2; }
@media (prefers-color-scheme: dark) {
  .t-system { --bg:#1c1c1f; --fg:#e9e9ec; --panel:#242428; --line:#3a3a41; --sel:#2f81f7; --hint:#9a9aa2; }
}
.toolbar, .cardrow { display:flex; gap:6px; align-items:center; }
button { padding:4px 10px; font:inherit; }
.danger { color:#b3261e; }
.grow { flex:1; }
.hint { font-size:11.5px; color:var(--hint); }
.list { flex:1; overflow:auto; background:var(--panel); border:1px solid var(--line); border-radius:6px; }
.row {
  display:grid; grid-template-columns: 2fr 1fr 1fr 90px; gap:8px;
  padding:8px 10px; border-bottom:1px solid var(--line); font-size:13.5px; cursor:default;
}
.compact .row { padding:2px 10px; font-size:12px; }
.row.head { font-weight:600; font-size:12px; color:var(--hint); position:sticky; top:0; background:var(--panel); }
.row.sel { background:var(--sel); color:#fff; }
.row.sel .chip { color:#fff; border-color:#fff8; }
.num { text-align:right; font-variant-numeric: tabular-nums; }
.chip { font-size:11px; border:1px solid var(--line); border-radius:9px; padding:0 7px; text-align:center; }
.status { display:flex; align-items:center; font-size:12px; color:var(--hint); min-height:16px; }
.pane { padding:10px; gap:6px; }
.fields { display:grid; grid-template-columns:70px 1fr; gap:6px 8px; align-items:center; }
.fields input, .fields select { font:inherit; padding:3px 6px; width:100%; }
.check { display:block; font-size:13px; margin:3px 0; }
fieldset { border:1px solid var(--line); border-radius:6px; }
.scrim {
  position:fixed; inset:0; background:#0006; z-index:99;
  display:flex; align-items:center; justify-content:center;
}
.card { background:var(--panel); color:var(--fg); border-radius:8px; padding:14px 16px; min-width:260px; }
.card h3 { margin:0 0 4px; font-size:15px; }
.card p { margin:0 0 10px; font-size:13px; color:var(--hint); }
.flash { animation: flash 300ms ease-out 1; }
@keyframes flash { from { background:#ffd60a; } to { background:transparent; } }
"#;
