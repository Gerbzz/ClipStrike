use std::cell::RefCell;

thread_local! {
    static TEST_COMMANDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static TEST_STATE: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn push_command(cmd: &str) {
    TEST_COMMANDS.with(|cmds| cmds.borrow_mut().push(cmd.to_string()));
}

pub fn pop_commands() -> Vec<String> {
    TEST_COMMANDS.with(|cmds| std::mem::take(&mut *cmds.borrow_mut()))
}

pub fn set_state(state: String) {
    TEST_STATE.with(|s| *s.borrow_mut() = state);
}

pub fn get_state() -> String {
    TEST_STATE.with(|s| s.borrow().clone())
}
