use super::*;

#[test]
fn environment_priority_requires_an_explicit_editor() -> Result<()> {
    let editor = EditorCommand::from_values(Some("nvim -f".into()), Some("ignored".into()))?;
    assert_eq!(editor.executable, "nvim");
    assert_eq!(editor.arguments, ["-f"]);
    let editor = EditorCommand::from_values(Some("  ".into()), Some("vim".into()))?;
    assert_eq!(editor.executable, "vim");
    assert!(EditorCommand::from_values(None, None).is_err());
    assert!(EditorCommand::from_values(Some("\t".into()), Some("".into())).is_err());
    // A malformed preferred setting is an error, not permission to choose another editor.
    assert!(EditorCommand::from_values(Some("'broken".into()), Some("vim".into())).is_err());
    Ok(())
}

#[test]
fn command_arguments_support_quotes_and_escapes_without_shell_expansion() -> Result<()> {
    let command = EditorCommand::parse(
        r#""/path with spaces/nvim" --wait 'two words' "" a\ b "$HOME" '$(touch nope)' | >"#,
    )?;
    assert_eq!(command.executable, "/path with spaces/nvim");
    assert_eq!(
        command.arguments,
        [
            "--wait",
            "two words",
            "",
            "a b",
            "$HOME",
            "$(touch nope)",
            "|",
            ">"
        ]
    );
    let command = command.command();
    assert_eq!(command.get_program(), "/path with spaces/nvim");
    assert_eq!(command.get_args().count(), 8);
    let command = EditorCommand::parse(r#"nvim "quote\"here" 'a\b' "a\b""#)?;
    assert_eq!(command.arguments, ["quote\"here", "a\\b", "a\\b"]);
    for invalid in ["", "   ", "''", "nvim 'broken", "nvim \"broken", "nvim \\"] {
        assert!(
            EditorCommand::parse(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
    Ok(())
}
