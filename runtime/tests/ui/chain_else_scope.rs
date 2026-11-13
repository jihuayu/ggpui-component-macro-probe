// error: E0425
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { if let Some(number) = source.get() && number > 0 {
        Text(number)
    } else { Text(number) } }
}
