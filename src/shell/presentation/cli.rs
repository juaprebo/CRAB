use crate::localization::domain::language::Language;
use crate::localization::presentation::messages::{message, text as tr};

pub fn print_formats(language: Language) {
    let external = crate::playback::infrastructure::ffmpeg::available();
    for format in crate::playback::infrastructure::formats::all() {
        if !format.external || external {
            println!(
                "{} ({}): {} [{}]",
                format.id,
                format.label,
                format.extensions,
                if format.external {
                    "FFmpeg"
                } else {
                    tr(language, "native")
                }
            );
        }
    }
    let status = tr(
        language,
        if external {
            "available"
        } else {
            "not installed; install ffmpeg for more codecs and formats"
        },
    );
    println!(
        "{}",
        message(
            language,
            "FFmpeg fallback: {status}",
            &[("status", status.into())]
        )
    );
    println!(
        "{}",
        tr(
            language,
            "Container support depends on the audio codec inside. FFmpeg support depends on its installed decoders."
        )
    );
}
