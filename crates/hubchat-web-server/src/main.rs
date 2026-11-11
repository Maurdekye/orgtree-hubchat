//! `hubchat-web`: Hubchat Web's server (spec section 9.1). It hands out the
//! web app's files, which are inside this program, and two small answers
//! (`/config.json`, `/healthz`). It keeps nothing, knows no one, and
//! connects to nothing: identities, keys and messages stay in each
//! person's browser, and the browser talks to the hub itself.
//!
//!   hubchat-web                 serve (settings: HUBCHAT_WEB_* in the environment)
//!   hubchat-web healthcheck     exit 0 when the server on this machine answers
//!   hubchat-web version         `hubchat-web <version>`
//!   hubchat-web files           the files inside, with their sizes
mod health;
mod serve;
mod settings;
mod site;

use std::process::ExitCode;

use settings::Settings;

fn settings() -> Result<Settings, ExitCode> {
    Settings::from_env(|name| std::env::var(name).ok()).map_err(|e| {
        eprintln!("hubchat-web: {e}");
        ExitCode::from(2)
    })
}

fn version() -> String {
    // a build without the real page must not pass for a release
    format!("hubchat-web {}{}", site::VERSION, if site::STAND_IN { " (stand-in page: built without HUBCHAT_WEB_DIST, not a release)" } else { "" })
}

async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        // `docker stop` sends SIGTERM; as the container's first process nothing else would end us
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn run() -> ExitCode {
    let settings = match settings() {
        Ok(s) => s,
        Err(code) => return code,
    };
    for note in &settings.notes {
        eprintln!("hubchat-web: {note}");
    }
    if site::STAND_IN {
        eprintln!("hubchat-web: this build holds a stand-in page, not Hubchat Web (it was built without HUBCHAT_WEB_DIST).");
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("hubchat-web: can't start: {e}");
            return ExitCode::from(1);
        }
    };
    runtime.block_on(async {
        let scheme = if settings.tls.is_some() { "https" } else { "http" };
        let suggesting = match (settings.hub_suggestions, settings.suggested_hubs.len()) {
            (false, _) => "no hub is suggested".to_string(),
            (true, 0) => "a hub at this server's own address is suggested when one answers".to_string(),
            (true, _) => format!("suggested hubs: {}", settings.suggested_hubs.join(", ")),
        };
        let serving = serve::serve(&settings, |at| eprintln!("hubchat-web {}: listening on {scheme}://{at} ({} files; {suggesting})", site::VERSION, site::FILES.len()));
        tokio::select! {
            r = serving => match r {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("hubchat-web: {e}");
                    ExitCode::from(1)
                }
            },
            () = stop_signal() => ExitCode::SUCCESS,
        }
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] | ["serve"] => run(),
        ["healthcheck"] => match settings().map(|s| health::check(&s)) {
            Ok(Ok(())) => ExitCode::SUCCESS,
            Ok(Err(e)) => {
                eprintln!("hubchat-web healthcheck: {e}");
                ExitCode::from(1)
            }
            Err(code) => code,
        },
        ["version"] | ["--version"] | ["-V"] => {
            println!("{}", version());
            ExitCode::SUCCESS
        }
        ["files"] => {
            for f in site::FILES {
                println!("{:>9} {:>9} {:>9}  {}", f.raw.len(), f.br.map_or("-".into(), |b| b.len().to_string()), f.gz.map_or("-".into(), |b| b.len().to_string()), f.path);
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: hubchat-web [serve | healthcheck | version | files]\nSettings come from the environment (HUBCHAT_WEB_PORT, HUBCHAT_WEB_BIND, HUBCHAT_WEB_TLS_CERT, HUBCHAT_WEB_TLS_KEY, HUBCHAT_WEB_SUGGEST_HUBS, HUBCHAT_WEB_HUB_SUGGESTIONS, HUBCHAT_WEB_NAME, HUBCHAT_WEB_LOG): docs/hubchat-web-server.md");
            ExitCode::from(2)
        }
    }
}
