// error: E0382
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(String::from("owned")));
    view! { match source.get() {
        whole @ Some(name) => Text(format!("{:?}:{}", whole.get(), name.get())),
        None => Text("none"),
    } }
}
