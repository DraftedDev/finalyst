use std::{
    fmt::Display,
    sync::{Arc, Mutex},
};

use futures::StreamExt;
use kdam::{Bar, BarExt, Column, RichProgress, Spinner};

#[derive(Clone)]
pub struct Progress(Arc<Mutex<RichProgress>>);

impl Progress {
    pub fn new(len: usize, label: impl Display) -> Self {
        let mut content = vec![
            Column::Text(" ".to_string()),
            Column::Spinner(Spinner::new(
                &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
                80.0,
                1.0,
            )),
            Column::Text(format!("[bold blue] {label}")),
        ];

        if len > 0 {
            content.extend([
                Column::Animation,
                Column::Text("•".to_string()),
                Column::Percentage(0),
                Column::Text("•".to_string()),
                Column::CountTotal,
                Column::Text("•".to_string()),
                Column::RemainingTime,
            ]);
        }

        Self(Arc::new(Mutex::new(RichProgress::new(
            Bar::new(len),
            content,
        ))))
    }

    pub fn inc(&self) {
        let mut progress = self.0.lock().expect("Failed to lock progress bar");
        progress.update(1).expect("Failed to update progress bar");
    }

    pub fn finish(self, label: impl Display) {
        self.0
            .lock()
            .expect("Failed to lock progress bar")
            .clear()
            .expect("Failed to clear progress bar");
        tracing::info!("{label}");
    }
}

pub async fn join_chunked<F, Fut, I, O>(
    iter: impl IntoIterator<Item = I>,
    chunk_size: usize,
    f: F,
) -> Vec<O>
where
    F: Fn(I) -> Fut,
    Fut: Future<Output = O>,
{
    futures::stream::iter(iter.into_iter().map(f))
        .buffered(chunk_size)
        .collect()
        .await
}
