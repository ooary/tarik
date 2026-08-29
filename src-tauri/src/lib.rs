use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    app_name: &'static str,
    app_version: &'static str,
    rust_target: &'static str,
}

fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_name: "Tarik",
        app_version: env!("CARGO_PKG_VERSION"),
        rust_target: std::env::consts::OS,
    }
}

#[tauri::command]
fn get_runtime_info() -> RuntimeInfo {
    runtime_info()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_runtime_info])
        .run(tauri::generate_context!())
        .expect("error while running Tarik");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_application_runtime() {
        let info = runtime_info();

        assert_eq!(info.app_name, "Tarik");
        assert_eq!(info.app_version, env!("CARGO_PKG_VERSION"));
        assert!(!info.rust_target.is_empty());
    }
}
