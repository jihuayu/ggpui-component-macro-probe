## Compile-time contract checks

These doctests must fail compilation; valid counterparts are integration tested.

Missing list key:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let rows = use_state(|| vec![1, 2]);
    view! { for row in rows.get() { Text(row) } }
}
```

Non-exhaustive match:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let flag = use_state(|| true);
    view! { match flag.get() { true => Text("yes") } }
}
```

Hook in an ordinary conditional:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    if true { let _state = use_state(|| 0); }
    view! { Text("bad") }
}
```

Hook in an event-like closure:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let _callback = || use_state(|| 0);
    view! { Text("bad") }
}
```

Async initialization:

```compile_fail
use probe_ui::prelude::*;
#[component]
async fn Bad() -> View { view! { Text("bad") } }
```

ReadProp cannot grant a setter:

```compile_fail
use probe_ui::prelude::*;
fn bad(prop: ReadProp<i32>) { prop.set(1); }
```

UI state cannot cross threads:

```compile_fail
use probe_ui::prelude::*;
fn bad(state: Signal<i32>) {
    std::thread::spawn(move || state.set(3));
}
```

Borrowed match payloads are outside this owned-data prototype:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(String::from("value")));
    view! {
        match source.get() {
            Some(ref name) => Text(name),
            None => Text("empty"),
        }
    }
}
```

Mutable payload bindings cannot be used as persistent read-only props:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { if let Some(mut value) = source.get() { Text(value) } }
}
```

KeepAlive requires one direct conditional:

```compile_fail
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    view! { KeepAlive(max_entries: 1) { Text("not a region") } }
}
```
