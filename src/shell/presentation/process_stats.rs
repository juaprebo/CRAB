use crate::localization::{domain::language::Language, presentation::messages::message};
use crate::shell::infrastructure::process_stats::{Stats, format_memory, format_uptime};

impl Stats {
    pub fn detail(&self, language: Language) -> String {
        let usage = self.reading();
        message(
            language,
            "CRAB uses {cpu}% of one core and {memory} of memory, running for {uptime}",
            &[
                ("cpu", format!("{:.1}", usage.cpu)),
                ("memory", format_memory(usage.memory)),
                ("uptime", format_uptime(usage.uptime)),
            ],
        )
    }
}
