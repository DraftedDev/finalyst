use futures::StreamExt;
use indicatif::ProgressStyle;
use tracing_indicatif::span_ext::IndicatifSpanExt;

pub async fn with_progress<Fut: Future<Output = R>, R>(
    msg: &str,
    len: u64,
    f: impl FnOnce(tracing::Span) -> Fut,
) -> R {
    let span = tracing::span!(tracing::Level::INFO, "progress");
    span.pb_set_message(msg);
    span.pb_set_length(len);

    let template = if len == 0 {
        "  [{spinner:.green}] {msg} │ {elapsed:<4}"
    } else {
        "  [{spinner:.green}] {msg} {wide_bar:.green/red} {pos}/{len} ({percent}%) │ {elapsed:<4}"
    };

    span.pb_set_style(
        &ProgressStyle::with_template(template)
            .unwrap()
            .progress_chars("━━━"),
    );

    let span2 = span.clone();
    let enter = span2.enter();
    let result = f(span).await;

    drop(enter);

    result
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
