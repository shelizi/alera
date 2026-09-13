pub use alera_core::agent_descriptor::AgentStartupPrompt;

#[cfg(test)]
mod tests {
    use alera_core::agent_descriptor::AGENT_DESCRIPTORS;

    use super::AgentStartupPrompt;

    #[test]
    fn every_detected_agent_has_a_shared_descriptor() {
        let types: Vec<_> = AGENT_DESCRIPTORS
            .iter()
            .map(|descriptor| descriptor.id)
            .collect();
        assert_eq!(
            types,
            [
                "codex",
                "claude",
                "copilot",
                "cursor",
                "agy",
                "opencode",
                "opencode2",
                "pi",
                "amp",
                "grok",
                "devin",
                "fx"
            ]
        );
        assert!(AGENT_DESCRIPTORS
            .iter()
            .all(|descriptor| !descriptor.default_command.is_empty()));
    }

    #[test]
    fn every_agent_declares_how_it_receives_its_initial_prompt() {
        let shapes: Vec<_> = AGENT_DESCRIPTORS
            .iter()
            .map(|descriptor| (descriptor.id, descriptor.startup_prompt))
            .collect();
        assert_eq!(
            shapes,
            [
                ("codex", AgentStartupPrompt::PositionalAfterTerminator),
                ("claude", AgentStartupPrompt::PositionalAfterTerminator),
                ("copilot", AgentStartupPrompt::LongOption("--interactive")),
                ("cursor", AgentStartupPrompt::PositionalAfterTerminator),
                (
                    "agy",
                    AgentStartupPrompt::LongOption("--prompt-interactive")
                ),
                ("opencode", AgentStartupPrompt::LongOption("--prompt")),
                ("opencode2", AgentStartupPrompt::LongOption("--prompt")),
                ("pi", AgentStartupPrompt::Positional),
                ("amp", AgentStartupPrompt::StdinScript),
                ("grok", AgentStartupPrompt::PositionalAfterTerminator),
                ("devin", AgentStartupPrompt::PositionalAfterTerminator),
                ("fx", AgentStartupPrompt::TerminalAfterReady),
            ]
        );
    }
}
