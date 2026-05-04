pub fn check_tool(name: &str) -> Result<(), String> {
    match std::process::Command::new(name).arg("--version").output() {
        Ok(_) => Ok(()),
        Err(_) => Err(format!("Required external tool '{}' not found in PATH", name)),
    }
}
