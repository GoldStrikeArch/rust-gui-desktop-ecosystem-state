//! "Fetcher" — async & network integration (SPEC-8), iced 0.14.
//!
//! Architecture notes (research-relevant):
//! - Runtime: iced's `tokio` feature makes the tokio runtime the app
//!   executor, so `Task` futures get a reactor (reqwest needs one). Futures
//!   never touch the UI thread: `update` returns a `Task`, the runtime polls
//!   it on tokio, and completed values come back as plain messages.
//! - Debounce + stale protection: every keystroke aborts the previous
//!   search task via `Task::abortable`'s `Handle` — the 250 ms
//!   `tokio::time::sleep` prefix makes the abort a debounce, and aborting
//!   after the sleep drops the in-flight reqwest future (real request
//!   cancellation). A generation counter on each response is kept as a
//!   belt-and-braces stale guard (`stale=` evidence in the selftest log).
//! - Download progress: iced 0.14's `sipper` feature (`Task::sip`) runs one
//!   task that *streams* progress (bytes received per `bytes_stream` chunk)
//!   and finishes with an output — both mapped to messages. `.abortable()`
//!   on that task gives the Cancel button; aborting drops the reqwest
//!   `Response` mid-stream, which closes the TCP connection (the server
//!   logs `ABORT /download` — the required proof).
//! - Flaky/retry: plain `Task::perform` + an error state with a Retry
//!   button; non-2xx handled via `error_for_status`.
//!
//! With `FETCH_SELFTEST=1`, request lifecycle evidence lines are printed to
//! stdout so synthetic input can be verified from a captured log.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use iced::futures::StreamExt;
use iced::task::{Handle, sipper};
use iced::widget::{
    button, column, container, progress_bar, row, scrollable, space, text,
    text_input,
};
use iced::{Border, Center, Color, Element, Fill, Task, Theme};
use serde::Deserialize;

const DOWNLOAD_SIZE: u64 = 8 * 1024 * 1024; // fallback if no Content-Length
static SELFTEST: AtomicBool = AtomicBool::new(false);

fn trace(line: impl FnOnce() -> String) {
    if SELFTEST.load(Ordering::Relaxed) {
        println!("{}", line());
    }
}

pub fn main() -> iced::Result {
    SELFTEST.store(
        std::env::var_os("FETCH_SELFTEST").is_some(),
        Ordering::Relaxed,
    );

    iced::application(Fetcher::new, Fetcher::update, Fetcher::view)
        .title(|_: &Fetcher| String::from("Fetcher (iced)"))
        .window_size((700.0, 560.0))
        .run()
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct SearchResult {
    id: u64,
    name: String,
    score: f64,
}

enum Download {
    Idle,
    Running {
        received: u64,
        total: u64,
        handle: Handle,
    },
    Done {
        mib: f64,
    },
    Cancelled {
        received: u64,
        total: u64,
    },
    Failed(String),
}

enum Flaky {
    Idle,
    Running { attempts: u32 },
    Failed { attempts: u32, error: String },
    Succeeded { attempts: u32, attempt: u64 },
}

struct Fetcher {
    base: String,
    client: reqwest::Client,
    // search
    query: String,
    searching: bool,
    results: Vec<SearchResult>,
    search_error: Option<String>,
    generation: u64,
    search_handle: Option<Handle>,
    // download
    download: Download,
    // flaky
    flaky: Flaky,
}

#[derive(Debug, Clone)]
enum Message {
    QueryChanged(String),
    SearchReady(u64, Result<Vec<SearchResult>, String>),
    StartDownload,
    DownloadProgress((u64, u64)),
    DownloadFinished(Result<u64, String>),
    CancelDownload,
    CallFlaky,
    FlakyReady(Result<u64, String>),
}

impl Fetcher {
    fn new() -> (Self, Task<Message>) {
        let port = std::env::var("FETCHER_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(7878);

        (
            Self {
                base: format!("http://127.0.0.1:{port}"),
                client: reqwest::Client::new(),
                query: String::new(),
                searching: false,
                results: Vec::new(),
                search_error: None,
                generation: 0,
                search_handle: None,
                download: Download::Idle,
                flaky: Flaky::Idle,
            },
            iced::widget::operation::focus("query"),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueryChanged(query) => {
                self.query = query;

                // Debounce + cancellation: kill the previous task, whether
                // it was still sleeping (debounce) or already on the wire.
                if let Some(handle) = self.search_handle.take() {
                    handle.abort();
                }
                self.generation += 1;

                if self.query.is_empty() {
                    self.searching = false;
                    self.results.clear();
                    self.search_error = None;
                    return Task::none();
                }

                self.searching = true;
                let generation = self.generation;
                let client = self.client.clone();
                let base = self.base.clone();
                let query = self.query.clone();

                trace(|| {
                    format!("SEARCH_QUEUED gen={generation} q={query:?}")
                });

                let (task, handle) = Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        let response = client
                            .get(format!("{base}/search"))
                            .query(&[("q", &query)])
                            .send()
                            .await
                            .map_err(|e| e.to_string())?
                            .error_for_status()
                            .map_err(|e| e.to_string())?;

                        response
                            .json::<Vec<SearchResult>>()
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| Message::SearchReady(generation, result),
                )
                .abortable();

                self.search_handle = Some(handle);
                return task;
            }
            Message::SearchReady(generation, result) => {
                // Stale guard: with Task abortion this should never fire for
                // an old generation, but it is kept (and logged) as evidence.
                let stale = generation != self.generation;
                trace(|| {
                    format!(
                        "SEARCH_READY gen={generation} stale={stale} {}",
                        match &result {
                            Ok(results) => format!("n={}", results.len()),
                            Err(error) => format!("err={error:?}"),
                        }
                    )
                });
                if stale {
                    return Task::none();
                }

                self.searching = false;
                match result {
                    Ok(results) => {
                        self.results = results;
                        self.search_error = None;
                    }
                    Err(error) => {
                        self.results.clear();
                        self.search_error = Some(error);
                    }
                }
            }
            Message::StartDownload => {
                let client = self.client.clone();
                let url = format!("{}/download", self.base);

                let download = sipper(move |mut progress| async move {
                    let response = client
                        .get(&url)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?
                        .error_for_status()
                        .map_err(|e| e.to_string())?;

                    let total =
                        response.content_length().unwrap_or(DOWNLOAD_SIZE);
                    let mut received: u64 = 0;
                    let mut stream = response.bytes_stream();

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.map_err(|e| e.to_string())?;
                        received += chunk.len() as u64;
                        progress.send((received, total)).await;
                    }

                    Ok(received)
                });

                let (task, handle) = Task::sip(
                    download,
                    Message::DownloadProgress,
                    Message::DownloadFinished,
                )
                .abortable();

                trace(|| String::from("DL_START"));
                self.download = Download::Running {
                    received: 0,
                    total: DOWNLOAD_SIZE,
                    handle,
                };
                return task;
            }
            Message::DownloadProgress((received, total)) => {
                if let Download::Running {
                    received: r,
                    total: t,
                    ..
                } = &mut self.download
                {
                    *r = received;
                    *t = total;
                    trace(|| format!("DL_PROGRESS {received}/{total}"));
                }
            }
            Message::DownloadFinished(result) => {
                self.download = match result {
                    Ok(bytes) => {
                        trace(|| format!("DL_DONE bytes={bytes}"));
                        Download::Done {
                            mib: bytes as f64 / (1024.0 * 1024.0),
                        }
                    }
                    Err(error) => {
                        trace(|| format!("DL_FAILED err={error:?}"));
                        Download::Failed(error)
                    }
                };
            }
            Message::CancelDownload => {
                if let Download::Running {
                    received,
                    total,
                    handle,
                } = std::mem::replace(&mut self.download, Download::Idle)
                {
                    // Aborting the task drops the in-flight reqwest response
                    // -> TCP close -> the server logs `ABORT /download`.
                    handle.abort();
                    trace(|| format!("DL_CANCELLED {received}/{total}"));
                    self.download = Download::Cancelled { received, total };
                }
            }
            Message::CallFlaky => {
                let attempts = match &self.flaky {
                    Flaky::Failed { attempts, .. } => *attempts,
                    _ => 0,
                } + 1;
                self.flaky = Flaky::Running { attempts };

                let client = self.client.clone();
                let url = format!("{}/flaky", self.base);

                #[derive(Deserialize)]
                struct Attempt {
                    attempt: u64,
                }

                return Task::perform(
                    async move {
                        let response = client
                            .get(&url)
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;

                        if !response.status().is_success() {
                            let status = response.status();
                            let body = response.text().await.unwrap_or_default();
                            return Err(format!("HTTP {status}: {body}"));
                        }

                        response
                            .json::<Attempt>()
                            .await
                            .map(|a| a.attempt)
                            .map_err(|e| e.to_string())
                    },
                    Message::FlakyReady,
                );
            }
            Message::FlakyReady(result) => {
                let attempts = match &self.flaky {
                    Flaky::Running { attempts } => *attempts,
                    _ => 1,
                };
                self.flaky = match result {
                    Ok(attempt) => {
                        trace(|| {
                            format!("FLAKY_OK attempts={attempts} server_attempt={attempt}")
                        });
                        Flaky::Succeeded { attempts, attempt }
                    }
                    Err(error) => {
                        trace(|| {
                            format!("FLAKY_ERR attempts={attempts} err={error:?}")
                        });
                        Flaky::Failed { attempts, error }
                    }
                };
            }
        }

        Task::none()
    }

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------

    fn view(&self) -> Element<'_, Message> {
        column![
            self.search_section(),
            self.download_section(),
            self.flaky_section(),
        ]
        .spacing(12)
        .padding(14)
        .into()
    }

    fn search_section(&self) -> Element<'_, Message> {
        let header = row![
            text("Search").size(15),
            space::horizontal(),
            if self.searching {
                text("searching…").size(12).style(text::secondary)
            } else {
                text(format!("{} results", self.results.len()))
                    .size(12)
                    .style(text::secondary)
            },
        ]
        .align_y(Center);

        let input = text_input("type to search (250 ms debounce)…", &self.query)
            .id("query")
            .on_input(Message::QueryChanged);

        let list: Element<'_, Message> =
            if let Some(error) = &self.search_error {
                container(
                    text(format!("search failed: {error}"))
                        .size(13)
                        .color(Color::from_rgb8(0xc2, 0x33, 0x2e)),
                )
                .padding(8)
                .into()
            } else {
                scrollable(
                    column(self.results.iter().map(|result| {
                        row![
                            text(format!("#{:03}", result.id))
                                .size(13)
                                .style(text::secondary)
                                .width(50),
                            text(&result.name).size(13),
                            space::horizontal(),
                            text(format!("{:.1}", result.score))
                                .size(13)
                                .style(text::secondary),
                        ]
                        .spacing(8)
                        .padding([2, 6])
                        .into()
                    }))
                    .width(Fill),
                )
                .height(Fill)
                .into()
            };

        section(column![header, input, list].spacing(8), true)
    }

    fn download_section(&self) -> Element<'_, Message> {
        const MIB: f64 = 1024.0 * 1024.0;

        let controls: Element<'_, Message> = match &self.download {
            Download::Running {
                received, total, ..
            } => {
                let ratio = *received as f32 / *total as f32;
                row![
                    progress_bar(0.0..=1.0, ratio).girth(16),
                    text(format!(
                        "{:.1} / {:.1} MiB",
                        *received as f64 / MIB,
                        *total as f64 / MIB
                    ))
                    .size(13)
                    .width(110),
                    button(text("Cancel").size(13))
                        .on_press(Message::CancelDownload)
                        .style(button::danger),
                ]
                .spacing(10)
                .align_y(Center)
                .into()
            }
            state => {
                let label: Element<'_, Message> = match state {
                    Download::Idle => text("8 MiB over ~8 s, streamed")
                        .size(13)
                        .style(text::secondary)
                        .into(),
                    Download::Done { mib } => {
                        text(format!("done — received {mib:.1} MiB"))
                            .size(13)
                            .color(Color::from_rgb8(0x1d, 0x7a, 0x33))
                            .into()
                    }
                    Download::Cancelled { received, total } => text(format!(
                        "cancelled at {:.1} / {:.1} MiB",
                        *received as f64 / MIB,
                        *total as f64 / MIB
                    ))
                    .size(13)
                    .color(Color::from_rgb8(0xc9, 0x8a, 0x0b))
                    .into(),
                    Download::Failed(error) => {
                        text(format!("failed: {error}"))
                            .size(13)
                            .color(Color::from_rgb8(0xc2, 0x33, 0x2e))
                            .into()
                    }
                    Download::Running { .. } => unreachable!(),
                };

                row![
                    button(text("Download").size(13))
                        .on_press(Message::StartDownload),
                    label,
                ]
                .spacing(10)
                .align_y(Center)
                .into()
            }
        };

        section(
            column![text("Download").size(15), controls].spacing(8),
            false,
        )
    }

    fn flaky_section(&self) -> Element<'_, Message> {
        let status: Element<'_, Message> = match &self.flaky {
            Flaky::Idle => text("fails twice, succeeds on the 3rd")
                .size(13)
                .style(text::secondary)
                .into(),
            Flaky::Running { attempts } => {
                text(format!("attempt {attempts}: calling…"))
                    .size(13)
                    .style(text::secondary)
                    .into()
            }
            Flaky::Failed { attempts, error } => row![
                text(format!("attempt {attempts} failed: {error}"))
                    .size(13)
                    .color(Color::from_rgb8(0xc2, 0x33, 0x2e)),
                button(text("Retry").size(13))
                    .on_press(Message::CallFlaky)
                    .style(button::danger),
            ]
            .spacing(10)
            .align_y(Center)
            .into(),
            Flaky::Succeeded { attempts, attempt } => text(format!(
                "success after {attempts} attempts (server counter {attempt})"
            ))
            .size(13)
            .color(Color::from_rgb8(0x1d, 0x7a, 0x33))
            .into(),
        };

        let call_enabled = !matches!(self.flaky, Flaky::Running { .. });

        section(
            column![
                text("Flaky endpoint").size(15),
                row![
                    button(text("Call /flaky").size(13)).on_press_maybe(
                        call_enabled.then_some(Message::CallFlaky)
                    ),
                    status,
                ]
                .spacing(10)
                .align_y(Center),
            ]
            .spacing(8),
            false,
        )
    }
}

fn section<'a>(
    content: impl Into<Element<'a, Message>>,
    fill: bool,
) -> Element<'a, Message> {
    let panel = container(content).padding(10).width(Fill).style(
        |theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.weakest.color.into()),
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            }
        },
    );

    if fill { panel.height(Fill).into() } else { panel.into() }
}
