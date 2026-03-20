use clipstrike_ui::test_hooks;

#[test]
fn test_hooks_are_thread_local() {
    test_hooks::push_command("main-command");
    test_hooks::set_state("main-state".to_string());

    let child = std::thread::spawn(|| {
        test_hooks::push_command("child-command");
        test_hooks::set_state("child-state".to_string());

        assert_eq!(
            test_hooks::pop_commands(),
            vec!["child-command".to_string()]
        );
        assert_eq!(test_hooks::get_state(), "child-state");
    });

    child.join().expect("child thread should not panic");

    assert_eq!(test_hooks::pop_commands(), vec!["main-command".to_string()]);
    assert_eq!(test_hooks::get_state(), "main-state");
}
