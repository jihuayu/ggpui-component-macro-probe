// error: E0716
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(String::from("owned")));
    view! { if let Some(name) = source.get().as_ref() { Text(name) } }
}
