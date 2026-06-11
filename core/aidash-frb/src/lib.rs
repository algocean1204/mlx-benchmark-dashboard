mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
mod api;
mod state;

pub use api::*;

/// FRB 기본 초기화 — codegen이 이 함수를 후킹한다.
#[flutter_rust_bridge::frb(init)]
pub fn init_frb() {
    flutter_rust_bridge::setup_default_user_utils();
}