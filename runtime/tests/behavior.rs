use probe_ui::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy)]
struct Flag(Signal<bool>);
#[derive(Clone, Default)]
struct Captures {
    states: Rc<RefCell<Vec<Signal<i32>>>>,
    disposed: Rc<RefCell<usize>>,
}
#[component]
fn Counter() -> View {
    let capture = use_context::<Captures>();
    let count = use_state(|| 0);
    capture.states.borrow_mut().push(count);
    on_dispose(move || *capture.disposed.borrow_mut() += 1);
    view! { Text(count.get()) }
}
#[component]
fn Conditional() -> View {
    let flag = use_context::<Flag>();
    view! { if flag.0.get() { Counter() } else { Text("closed") } }
}
#[component]
fn Cached() -> View {
    let flag = use_context::<Flag>();
    view! { KeepAlive(max_entries: 1) { if flag.0.get() { Counter() } } }
}
fn setup(open: bool) -> (Engine, Signal<bool>, Captures) {
    let app = Engine::new();
    let flag = app.source(open);
    let capture = Captures::default();
    app.provide(Flag(flag));
    app.provide(capture.clone());
    (app, flag, capture)
}

#[test]
fn component_is_initialized_once_and_local_changes_only_update_text() {
    let (app, _, capture) = setup(true);
    app.mount(Conditional).unwrap();
    let count = capture.states.borrow()[0];
    count.set(3);
    app.flush().unwrap();
    assert_eq!(app.text(), ["3"]);
    assert_eq!(app.stats().mounts["Counter"], 1);
    assert_eq!(app.stats().mounts["Conditional"], 1);
}

#[test]
fn if_disposes_and_remounts_fresh_state_and_rejects_stale_handle() {
    let (app, flag, capture) = setup(true);
    app.mount(Conditional).unwrap();
    let old = capture.states.borrow()[0];
    old.set(8);
    flag.set(false);
    app.flush().unwrap();
    assert_eq!(app.text(), ["closed"]);
    assert_eq!(*capture.disposed.borrow(), 1);
    assert_eq!(old.try_set(99), Err(Error::Disposed));
    flag.set(true);
    app.flush().unwrap();
    assert_eq!(app.text(), ["0"]);
    assert_eq!(app.stats().mounts["Counter"], 2);
}

#[test]
fn batched_flip_to_original_value_does_not_remount() {
    let (app, flag, capture) = setup(true);
    app.mount(Conditional).unwrap();
    capture.states.borrow()[0].set(4);
    flag.set(false);
    flag.set(true);
    app.flush().unwrap();
    assert_eq!(app.text(), ["4"]);
    assert_eq!(app.stats().mounts["Counter"], 1);
}

#[test]
fn equal_writes_do_not_schedule_work() {
    let (app, flag, _) = setup(true);
    app.mount(Conditional).unwrap();
    let before = app.stats().watcher_runs;
    assert_eq!(flag.try_set(true), Ok(false));
    app.flush().unwrap();
    assert_eq!(app.stats().watcher_runs, before);
}

#[test]
fn two_siblings_have_independent_state() {
    let (app, _, capture) = setup(true);
    app.mount(|| view! { Counter(), Counter() }).unwrap();
    capture.states.borrow()[0].set(6);
    app.flush().unwrap();
    assert_eq!(app.text(), ["6", "0"]);
}

#[test]
fn separate_app_providers_and_state_do_not_mix() {
    let (first, _, a) = setup(true);
    let (second, _, b) = setup(true);
    first.mount(Conditional).unwrap();
    second.mount(Conditional).unwrap();
    a.states.borrow()[0].set(6);
    b.states.borrow()[0].set(9);
    first.flush().unwrap();
    second.flush().unwrap();
    assert_eq!(first.text(), ["6"]);
    assert_eq!(second.text(), ["9"]);
}

#[test]
fn keep_alive_preserves_state_and_parent_unmount_disposes_it() {
    let (app, flag, capture) = setup(true);
    app.mount(Cached).unwrap();
    let count = capture.states.borrow()[0];
    count.set(7);
    app.flush().unwrap();
    flag.set(false);
    app.flush().unwrap();
    assert!(app.text().is_empty());
    assert_eq!(*capture.disposed.borrow(), 0);
    assert_eq!(count.try_set(8), Err(Error::Inactive));
    flag.set(true);
    app.flush().unwrap();
    assert_eq!(app.text(), ["7"]);
    assert_eq!(app.stats().mounts["Counter"], 1);
    app.unmount();
    assert_eq!(count.try_get(), Err(Error::Disposed));
    assert_eq!(*capture.disposed.borrow(), 1);
    assert_eq!(app.stats().live_watchers, 0);
}

#[derive(Clone, PartialEq)]
enum Phase {
    Idle,
    Ready { name: String },
    Failed { message: String },
}
#[derive(Clone, Copy)]
struct PhaseSource(Signal<Phase>);
#[component]
fn Payload(name: ReadProp<String>) -> View {
    let capture = use_context::<Captures>();
    let count = use_state(|| 0);
    capture.states.borrow_mut().push(count);
    view! { Text(format!("{}:{}", name.get(), count.get())) }
}
#[component]
fn Matched() -> View {
    let phase = use_context::<PhaseSource>().0;
    view! {
        match phase.get() {
            Phase::Idle => Text("idle"),
            Phase::Ready { name } => Payload(name: name),
            Phase::Failed { message } => Text(message),
        }
    }
}

#[test]
fn match_same_arm_updates_payload_without_remounting() {
    let (app, _, capture) = setup(true);
    let phase = app.source(Phase::Ready { name: "A".into() });
    app.provide(PhaseSource(phase));
    app.mount(Matched).unwrap();
    capture.states.borrow()[0].set(5);
    phase.set(Phase::Ready { name: "B".into() });
    app.flush().unwrap();
    assert_eq!(app.text(), ["B:5"]);
    assert_eq!(app.stats().mounts["Payload"], 1);
    assert_eq!(app.stats().mounts["Matched"], 1);
}

#[test]
fn match_change_arm_disposes_payload_watchers_before_they_evaluate() {
    let (app, _, capture) = setup(true);
    let phase = app.source(Phase::Ready { name: "A".into() });
    app.provide(PhaseSource(phase));
    app.mount(Matched).unwrap();
    let old = capture.states.borrow()[0];
    phase.set(Phase::Failed {
        message: "error".into(),
    });
    app.flush().unwrap();
    assert_eq!(app.text(), ["error"]);
    assert_eq!(old.try_get(), Err(Error::Disposed));
    phase.set(Phase::Idle);
    app.flush().unwrap();
    assert_eq!(app.text(), ["idle"]);
}

#[component]
fn Guarded() -> View {
    let flag = use_context::<Flag>().0;
    let phase = use_context::<PhaseSource>().0;
    view! {
        match phase.get() {
            Phase::Ready { name } if flag.get() && name.starts_with('A') =>
                Payload(name: name),
            Phase::Ready { name } => Text(format!("fallback:{}", name.get())),
            _ => Text("other"),
        }
    }
}
#[test]
fn match_guards_are_reactive_and_use_snapshot_bindings() {
    let (app, flag, capture) = setup(true);
    let phase = app.source(Phase::Ready { name: "A".into() });
    app.provide(PhaseSource(phase));
    app.mount(Guarded).unwrap();
    let old = capture.states.borrow()[0];
    flag.set(false);
    app.flush().unwrap();
    assert_eq!(app.text(), ["fallback:A"]);
    assert_eq!(old.try_get(), Err(Error::Disposed));
}

#[derive(Clone, Copy)]
struct Flags {
    outer: Signal<bool>,
    inner: Signal<bool>,
}
#[component]
fn ShortCircuit() -> View {
    let flags = use_context::<Flags>();
    view! { if flags.outer.get() && flags.inner.get() { Text("yes") } else { Text("no") } }
}
#[test]
fn dynamic_dependencies_are_unsubscribed_when_no_longer_read() {
    let app = Engine::new();
    let flags = Flags {
        outer: app.source(true),
        inner: app.source(true),
    };
    app.provide(flags);
    app.mount(ShortCircuit).unwrap();
    flags.outer.set(false);
    app.flush().unwrap();
    let before = app.stats().watcher_runs;
    flags.inner.set(false);
    app.flush().unwrap();
    assert_eq!(app.stats().watcher_runs, before);
}

#[component]
fn NestedCache() -> View {
    let flags = use_context::<Flags>();
    view! {
        KeepAlive(max_entries: 1) {
            if flags.outer.get() {
                KeepAlive(max_entries: 1) {
                    if flags.inner.get() { Counter() }
                }
            }
        }
    }
}
#[test]
fn reactivating_parent_does_not_activate_a_hidden_cached_grandchild() {
    let (app, _, capture) = setup(true);
    let flags = Flags {
        outer: app.source(true),
        inner: app.source(true),
    };
    app.provide(flags);
    app.mount(NestedCache).unwrap();
    let count = capture.states.borrow()[0];
    count.set(9);
    flags.inner.set(false);
    app.flush().unwrap();
    flags.outer.set(false);
    app.flush().unwrap();
    flags.outer.set(true);
    app.flush().unwrap();
    assert!(app.text().is_empty());
    assert_eq!(count.try_get(), Err(Error::Inactive));
    flags.inner.set(true);
    app.flush().unwrap();
    assert_eq!(app.text(), ["9"]);
}

#[derive(Clone, PartialEq)]
struct Row {
    id: u64,
    label: String,
}
fn rows() -> Vec<Row> {
    vec![
        Row {
            id: 1,
            label: "A".into(),
        },
        Row {
            id: 2,
            label: "B".into(),
        },
    ]
}
#[derive(Clone, Copy)]
struct Rows(Signal<Vec<Row>>);
type RowStateEntries = Rc<RefCell<Vec<(u64, Signal<i32>)>>>;
#[derive(Clone, Default)]
struct RowStates(RowStateEntries);
#[component]
fn Item(row: ReadProp<Row>) -> View {
    let states = use_context::<RowStates>();
    let count = use_state(|| 0);
    states.0.borrow_mut().push((row.get().id, count));
    view! { Text(format!("{}:{}:{}", row.get().id, row.get().label, count.get())) }
}
#[component]
fn List() -> View {
    let rows = use_context::<Rows>().0;
    view! { for row in rows.get() key row.id { Item(row: row) } }
}
fn list_setup() -> (Engine, Signal<Vec<Row>>, RowStates) {
    let app = Engine::new();
    let source = app.source(rows());
    let states = RowStates::default();
    app.provide(Rows(source));
    app.provide(states.clone());
    app.mount(List).unwrap();
    (app, source, states)
}
#[test]
fn keyed_reorder_and_same_key_new_data_keep_row_state() {
    let (app, source, states) = list_setup();
    states.0.borrow()[0].1.set(7);
    source.update(|rows| {
        rows[0].label = "renamed".into();
        rows.reverse();
    });
    app.flush().unwrap();
    assert_eq!(app.text(), ["2:B:0", "1:renamed:7"]);
    assert_eq!(app.stats().mounts["Item"], 2);
    assert_eq!(app.stats().mounts["List"], 1);
}
#[test]
fn filtering_and_reinserting_the_same_key_resets_state() {
    let (app, source, states) = list_setup();
    let old = states.0.borrow()[0].1;
    old.set(7);
    source.update(|rows| {
        rows.remove(0);
    });
    app.flush().unwrap();
    assert_eq!(old.try_get(), Err(Error::Disposed));
    source.set(rows());
    app.flush().unwrap();
    assert_eq!(app.text(), ["1:A:0", "2:B:0"]);
    assert_eq!(app.stats().mounts["Item"], 3);
}
#[test]
fn duplicate_keys_reject_the_whole_list_before_any_changes() {
    let (app, source, _) = list_setup();
    let before = app.stats();
    source.set(vec![
        Row {
            id: 1,
            label: "bad".into(),
        },
        Row {
            id: 1,
            label: "duplicate".into(),
        },
    ]);
    assert_eq!(app.flush(), Err(Error::DuplicateKey("1".into())));
    assert_eq!(app.text(), ["1:A:0", "2:B:0"]);
    assert_eq!(app.stats().mounts, before.mounts);
    assert_eq!(app.stats().disposals, before.disposals);
    source.set(rows());
    app.flush().unwrap();
    assert_eq!(app.text(), ["1:A:0", "2:B:0"]);
}
#[test]
fn changing_key_remounts_but_other_rows_are_untouched() {
    let (app, source, states) = list_setup();
    let old = states.0.borrow()[0].1;
    source.update(|rows| rows[0].id = 3);
    app.flush().unwrap();
    assert_eq!(old.try_get(), Err(Error::Disposed));
    assert_eq!(app.stats().mounts["Item"], 3);
    assert_eq!(app.text(), ["3:A:0", "2:B:0"]);
}
#[test]
fn unflushed_remove_and_reinsert_uses_final_key_set() {
    let (app, source, states) = list_setup();
    states.0.borrow()[0].1.set(7);
    source.set(Vec::new());
    source.set(rows());
    app.flush().unwrap();
    assert_eq!(app.text(), ["1:A:7", "2:B:0"]);
    assert_eq!(app.stats().mounts["Item"], 2);
}

#[derive(Clone, PartialEq)]
struct Model {
    count: i32,
    doubled: i32,
}
struct ModelStore;
impl StoreKey for ModelStore {
    type State = Model;
}
struct Increment;
impl Command for Increment {
    type Output = ();
    type Error = ();
}
#[derive(Clone, Default)]
struct CommandCapture(Rc<RefCell<Option<CommandHandle<Increment>>>>);
#[component]
fn StorePage() -> View {
    let store = use_store::<ModelStore>();
    let command = use_command::<Increment>();
    let capture = use_context::<CommandCapture>();
    *capture.0.borrow_mut() = Some(command);
    let count = store.select(|s| s.count);
    let doubled = store.select(|s| s.doubled);
    view! { Text(format!("{}:{}", count.get(), doubled.get())) }
}
#[test]
fn use_store_and_typed_command_update_bindings_without_rebuilding_component() {
    let app = Engine::new();
    let writer = app.store::<ModelStore>(Model {
        count: 0,
        doubled: 0,
    });
    app.command::<Increment>(move |_| {
        writer.update(|s| {
            s.count += 1;
            s.doubled = s.count * 2;
        });
        Ok(())
    });
    let capture = CommandCapture::default();
    app.provide(capture.clone());
    app.mount(StorePage).unwrap();
    capture
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .send(Increment)
        .unwrap();
    app.flush().unwrap();
    assert_eq!(app.text(), ["1:2"]);
    assert_eq!(app.stats().mounts["StorePage"], 1);
}

#[test]
fn many_mount_unmount_cycles_release_scopes_signals_and_watchers() {
    let (app, flag, _) = setup(false);
    app.mount(Conditional).unwrap();
    let baseline = app.stats();
    for _ in 0..300 {
        flag.set(true);
        app.flush().unwrap();
        flag.set(false);
        app.flush().unwrap();
    }
    let final_stats = app.stats();
    assert_eq!(baseline.live_scopes, final_stats.live_scopes);
    assert_eq!(baseline.live_signals, final_stats.live_signals);
    assert_eq!(baseline.live_watchers, final_stats.live_watchers);
    assert_eq!(final_stats.mounts["Counter"], 300);
    assert_eq!(final_stats.disposals["Counter"], 300);
}

#[component]
fn Panics() -> View {
    let _state = use_state(|| 1);
    panic!("intentional construction failure");
}
#[test]
fn panic_restores_current_scope_and_cleans_failed_mount() {
    let (app, _, capture) = setup(true);
    let before = app.stats();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.mount(Panics)));
    assert!(caught.is_err());
    assert_eq!(app.stats().live_scopes, before.live_scopes);
    assert_eq!(app.stats().live_signals, before.live_signals);
    assert!(std::panic::catch_unwind(|| use_state(|| 1)).is_err());
    app.mount(Counter).unwrap();
    assert_eq!(capture.states.borrow().len(), 1);
}

#[test]
fn hooks_outside_component_initialization_are_rejected() {
    let app = Engine::new();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.mount(|| view! { Text(use_state(|| 1).get()) })
    }));
    assert!(caught.is_err());
    assert_eq!(app.stats().live_scopes, 1);
}

#[test]
fn mutation_in_tracked_condition_is_rejected() {
    let app = Engine::new();
    let source = app.source(true);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.mount(|| view! { if { source.set(false); true } { Text("bad") } })
    }));
    assert!(caught.is_err());
    assert_eq!(app.stats().live_scopes, 1);
}

#[derive(Clone, Copy)]
struct Choice(Signal<u8>);
#[component]
fn CacheArms() -> View {
    let choice = use_context::<Choice>().0;
    view! {
        KeepAlive(max_entries: 1) {
            match choice.get() {
                0 => Counter(),
                1 => Counter(),
                _ => Counter(),
            }
        }
    }
}
#[test]
fn cache_capacity_evicts_oldest_inactive_arm() {
    let (app, _, capture) = setup(true);
    let choice = app.source(0);
    app.provide(Choice(choice));
    app.mount(CacheArms).unwrap();
    let oldest = capture.states.borrow()[0];
    oldest.set(7);
    choice.set(1);
    app.flush().unwrap();
    choice.set(2);
    app.flush().unwrap();
    assert_eq!(oldest.try_get(), Err(Error::Disposed));
    assert_eq!(*capture.disposed.borrow(), 1);
    choice.set(0);
    app.flush().unwrap();
    assert_eq!(app.text(), ["0"]);
    assert_eq!(app.stats().mounts["Counter"], 4);
}

#[component]
fn CachedPayload() -> View {
    let flag = use_context::<Flag>().0;
    let phase = use_context::<PhaseSource>().0;
    view! {
        KeepAlive(max_entries: 1) {
            if flag.get() {
                match phase.get() {
                    Phase::Ready { name } => Payload(name: name),
                    _ => Text("other"),
                }
            }
        }
    }
}
#[test]
fn hidden_cached_scope_does_not_recompute_and_refreshes_latest_data_on_resume() {
    let (app, flag, capture) = setup(true);
    let phase = app.source(Phase::Ready { name: "A".into() });
    app.provide(PhaseSource(phase));
    app.mount(CachedPayload).unwrap();
    capture.states.borrow()[0].set(3);
    flag.set(false);
    app.flush().unwrap();
    let before = app.stats().watcher_runs;
    phase.set(Phase::Ready { name: "B".into() });
    app.flush().unwrap();
    assert_eq!(app.stats().watcher_runs, before);
    flag.set(true);
    app.flush().unwrap();
    assert_eq!(app.text(), ["B:3"]);
    assert_eq!(app.stats().mounts["Payload"], 1);
}

#[derive(Clone, Copy)]
struct TwoLists {
    left: Signal<Vec<Row>>,
    right: Signal<Vec<Row>>,
}
#[component]
fn Lists() -> View {
    let lists = use_context::<TwoLists>();
    view! {
        for row in lists.left.get() key row.id { Item(row: row) },
        for row in lists.right.get() key row.id { Item(row: row) },
    }
}
#[test]
fn moving_same_key_between_lists_creates_new_instance() {
    let app = Engine::new();
    let lists = TwoLists {
        left: app.source(vec![rows()[0].clone()]),
        right: app.source(Vec::new()),
    };
    let states = RowStates::default();
    app.provide(lists);
    app.provide(states.clone());
    app.mount(Lists).unwrap();
    let old = states.0.borrow()[0].1;
    old.set(8);
    lists.left.set(Vec::new());
    lists.right.set(vec![rows()[0].clone()]);
    app.flush().unwrap();
    assert_eq!(app.text(), ["1:A:0"]);
    assert_eq!(old.try_get(), Err(Error::Disposed));
}

#[component]
fn SnapshotVsBinding() -> View {
    let flag = use_context::<Flag>().0;
    let snapshot = flag.get();
    let memo = use_memo(move || if flag.get() { "on" } else { "off" });
    view! { Text(snapshot), Text(flag.get()), Text(memo) }
}
#[test]
fn plain_rust_snapshot_stays_fixed_while_view_and_memo_are_reactive() {
    let (app, flag, _) = setup(true);
    app.mount(SnapshotVsBinding).unwrap();
    flag.set(false);
    app.flush().unwrap();
    assert_eq!(app.text(), ["true", "false", "off"]);
    assert_eq!(app.stats().mounts["SnapshotVsBinding"], 1);
}
