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

#[test]
fn both_real_commands_implement_private_official_identity_protocol() {
    let expected = network_tui::update::BuildIdentity::official();
    if option_env!("LINKLENS_RELEASE_VERSION").is_some()
        || option_env!("LINKLENS_RELEASE_TARGET").is_some()
    {
        assert!(
            expected.is_ok(),
            "带发布标识的构建必须具有有效官方身份：{expected:?}"
        );
    }
    for command in [env!("CARGO_BIN_EXE_linklens"), env!("CARGO_BIN_EXE_llens")] {
        let output = Command::new(command)
            .arg("--linklens-update-identity")
            .output()
            .unwrap();
        match &expected {
            Ok(build) => {
                assert!(output.status.success());
                let actual: network_tui::update::BuildIdentity =
                    serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(&actual, build);
                assert!(output.stderr.is_empty());
            }
            Err(error) => {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains(error));
                assert!(output.stdout.is_empty());
            }
        }
    }
}
