// Evidence probe (verification code): what happens if a reqwest future is
// polled on Slint's event loop via slint::spawn_local, without a tokio
// runtime? Expected: panic ("there is no reactor running / must be called
// from the context of a Tokio runtime") — proving that the
// background-tokio-thread architecture in main.rs is required, not a choice
// of taste. Run: cargo run --release --bin spawnlocal_probe
slint::slint! {
    export component Probe inherits Window {
        title: "spawn_local probe";
        width: 200px;
        height: 60px;
        Text { text: "probing…"; }
    }
}

fn main() {
    let port = std::env::var("FETCHER_PORT").unwrap_or_else(|_| "7878".into());
    let url = format!("http://127.0.0.1:{port}/health");
    let probe = Probe::new().unwrap();
    probe.show().unwrap();
    slint::spawn_local(async move {
        println!("PROBE polling reqwest future on the Slint event loop…");
        match reqwest::get(&url).await {
            Ok(r) => println!("PROBE_UNEXPECTED_SUCCESS status={}", r.status()),
            Err(e) => println!("PROBE_ERR {e}"),
        }
        let _ = slint::quit_event_loop();
    })
    .unwrap();
    // safety net: quit after 5 s if nothing happened
    let t = slint::Timer::default();
    t.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(5), || {
        println!("PROBE_TIMEOUT");
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop().unwrap();
}
