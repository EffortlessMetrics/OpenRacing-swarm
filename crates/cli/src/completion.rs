//! Shell completion generation for wheelctl

use clap::CommandFactory;
use clap_complete::{Shell, generate};
use std::io;

use crate::Cli;

/// Generate shell completion script
pub fn generate_completion(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = "wheelctl";

    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}

/// Print installation instructions for completions
#[allow(dead_code)]
pub fn print_completion_instructions(shell: Shell) {
    match shell {
        Shell::Bash | Shell::Zsh | Shell::Fish | Shell::PowerShell => {
            let lines = completion_instructions::for_shell(shell);
            completion_instructions::print_lines(lines);
        }
        _ => completion_instructions::print_generic(shell),
    }
}

mod completion_instructions {
    use clap_complete::Shell;

    pub(super) fn for_shell(shell: Shell) -> &'static [&'static str] {
        match shell {
            Shell::Bash => bash_lines(),
            Shell::Zsh => zsh_lines(),
            Shell::Fish => fish_lines(),
            Shell::PowerShell => powershell_lines(),
            _ => &[],
        }
    }

    pub(super) fn print_lines(lines: &[&str]) {
        for line in lines {
            println!("{line}");
        }
    }

    fn bash_lines() -> &'static [&'static str] {
        &[
            "# Add this to your ~/.bashrc:",
            "eval \"$(wheelctl completion bash)\"",
            "",
            "# Or save to a file and source it:",
            "wheelctl completion bash > ~/.wheelctl-completion.bash",
            "echo 'source ~/.wheelctl-completion.bash' >> ~/.bashrc",
        ]
    }

    fn zsh_lines() -> &'static [&'static str] {
        &[
            "# Add this to your ~/.zshrc:",
            "eval \"$(wheelctl completion zsh)\"",
            "",
            "# Or save to a file in your fpath:",
            "wheelctl completion zsh > ~/.zsh/completions/_wheelctl",
            "# Make sure ~/.zsh/completions is in your fpath",
        ]
    }

    fn fish_lines() -> &'static [&'static str] {
        &[
            "# Save completion to fish completions directory:",
            "wheelctl completion fish > ~/.config/fish/completions/wheelctl.fish",
        ]
    }

    fn powershell_lines() -> &'static [&'static str] {
        &[
            "# Add this to your PowerShell profile:",
            "Invoke-Expression (& wheelctl completion powershell | Out-String)",
            "",
            "# Or save to a file and dot-source it:",
            "wheelctl completion powershell > wheelctl-completion.ps1",
            ". ./wheelctl-completion.ps1",
        ]
    }

    pub(super) fn print_generic(shell: Shell) {
        println!("Completion generated for {shell:?}");
        println!("Please refer to your shell's documentation for installation instructions.");
    }
}
