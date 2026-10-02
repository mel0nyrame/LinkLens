use std::process::Command;

#[test]
fn both_commands_reject_unknown_arguments_without_entering_tui() {
    for command in [env!("CARGO_BIN_EXE_linklens"), env!("CARGO_BIN_EXE_llens")] {
        let output = Command::new(command).arg("unknown").output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("不支持的参数"), "{error}");
        assert!(!output.stdout.windows(8).any(|part| part == b"\x1b[?1049h"));
    }
}

#[test]
fn source_update_is_equivalent_and_does_not_enter_tui_even_if_checks_are_disabled() {
    if network_tui::update::BuildIdentity::official().is_ok() {
        return;
    }
    let mut errors = Vec::new();
    for command in [env!("CARGO_BIN_EXE_linklens"), env!("CARGO_BIN_EXE_llens")] {
        let output = Command::new(command)
            .arg("update")
            .env("LINKLENS_NO_UPDATE_CHECK", "1")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("源码构建不支持自更新"), "{error}");
        assert!(output.stdout.is_empty());
        errors.push(error);
    }
    assert_eq!(errors[0], errors[1]);
}
