use std::fmt::Display;

use kdam::{Bar, Column, RichProgress, Spinner};

pub fn progress_bar(len: usize, label: impl Display) -> RichProgress {
    RichProgress::new(
        Bar::new(len),
        vec![
            Column::Text(" ".to_string()),
            Column::Spinner(Spinner::new(
                &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
                80.0,
                1.0,
            )),
            Column::Text(format!("[bold blue] {label}")),
            Column::Animation,
            Column::Percentage(0),
            Column::Text("•".to_string()),
            Column::CountTotal,
            Column::Text("•".to_string()),
            Column::RemainingTime,
        ],
    )
}
