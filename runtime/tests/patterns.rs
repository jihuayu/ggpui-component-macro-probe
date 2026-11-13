//! Owned reactive pattern contracts exercised through the actual proc macros.
use probe_ui::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
struct Captures(Rc<RefCell<Vec<Signal<i32>>>>);
#[derive(Clone, Copy)]
struct Optional(Signal<Option<String>>);
#[component]
fn Stateful(value: ReadProp<String>) -> View {
    let local = use_state(|| 0);
    use_context::<Captures>().0.borrow_mut().push(local);
    view! { Text(format!("{}:{}", value.get(), local.get())) }
}
#[component]
fn IfLet() -> View {
    let source = use_context::<Optional>().0;
    view! {
        if let Some(name) = source.get() {
            Stateful(value: name)
        } else {
            Text("none")
        }
    }
}
fn optional() -> (Engine, Signal<Option<String>>, Captures) {
    let app = Engine::new();
    let source = app.source(Some("A".into()));
    let captures = Captures::default();
    app.provide(Optional(source));
    app.provide(captures.clone());
    (app, source, captures)
}
#[test]
fn if_let_updates_owned_payload_and_preserves_state_until_pattern_fails() {
    let (app, source, captures) = optional();
    app.mount(IfLet).unwrap();
    let old = captures.0.borrow()[0];
    old.set(7);
    source.set(Some("B".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["B:7"]);
    assert_eq!(app.stats().mounts["Stateful"], 1);
    source.set(None);
    app.flush().unwrap();
    assert_eq!(app.text(), ["none"]);
    assert_eq!(old.try_get(), Err(Error::Disposed));
    source.set(Some("C".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["C:0"]);
}

#[component]
fn IfLetNoElse() -> View {
    let source = use_context::<Optional>().0;
    view! { if let Some(value) = source.get() { Text(value) } }
}
#[test]
fn if_let_without_else_retains_an_empty_region() {
    let (app, source, _) = optional();
    source.set(None);
    app.mount(IfLetNoElse).unwrap();
    assert!(app.text().is_empty());
    source.set(Some("ready".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["ready"]);
    source.set(None);
    app.flush().unwrap();
    assert!(app.text().is_empty());
}

#[derive(Clone, PartialEq)]
struct User {
    name: String,
    enabled: bool,
    quota: Option<u32>,
}
#[derive(Clone, Copy)]
struct Session {
    allowed: Signal<bool>,
    user: Signal<Option<User>>,
    minimum: Signal<u32>,
}
fn user(name: &str, quota: Option<u32>) -> Option<User> {
    Some(User {
        name: name.into(),
        enabled: true,
        quota,
    })
}
#[component]
fn Chain() -> View {
    let session = use_context::<Session>();
    view! {
        if session.allowed.get()
            && let Some(user) = session.user.get()
            && user.enabled
            && let Some(quota) = user.quota
            && quota >= session.minimum.get()
        {
            Text(format!("{}:{}", user.get().name, quota.get()))
        } else {
            Text("denied")
        }
    }
}
fn session() -> (Engine, Session) {
    let app = Engine::new();
    let session = Session {
        allowed: app.source(true),
        user: app.source(user("Ada", Some(10))),
        minimum: app.source(5),
    };
    app.provide(session);
    (app, session)
}
#[test]
fn let_chains_support_boolean_prefixes_multiple_bindings_and_payload_updates() {
    let (app, session) = session();
    app.mount(Chain).unwrap();
    assert_eq!(app.text(), ["Ada:10"]);
    session.user.set(user("Grace", Some(20)));
    app.flush().unwrap();
    assert_eq!(app.text(), ["Grace:20"]);
    session.user.set(user("Grace", None));
    app.flush().unwrap();
    assert_eq!(app.text(), ["denied"]);
}
#[test]
fn let_chains_short_circuit_and_drop_unreachable_dependencies() {
    let (app, session) = session();
    session.allowed.set(false);
    app.mount(Chain).unwrap();
    let before = app.stats().watcher_runs;
    session.user.set(user("hidden", Some(20)));
    session.minimum.set(12);
    app.flush().unwrap();
    assert_eq!(app.stats().watcher_runs, before);
    session.allowed.set(true);
    app.flush().unwrap();
    assert_eq!(app.text(), ["hidden:20"]);
    session.user.set(None);
    app.flush().unwrap();
    let before = app.stats().watcher_runs;
    session.minimum.set(99);
    app.flush().unwrap();
    assert_eq!(app.stats().watcher_runs, before);
}

#[derive(Clone, Copy)]
struct Alternatives {
    first: Signal<Option<String>>,
    second: Signal<Option<String>>,
    flag: Signal<bool>,
}
#[component]
fn ElseChain() -> View {
    let choices = use_context::<Alternatives>();
    view! {
        if let Some(name) = choices.first.get() {
            Text(format!("first:{}", name.get()))
        } else if choices.flag.get() && let Some(name) = choices.second.get() {
            Text(format!("second:{}", name.get()))
        } else {
            Text("empty")
        }
    }
}
#[test]
fn else_if_let_can_shadow_branch_bindings_and_reactivate_with_latest_value() {
    let app = Engine::new();
    let choices = Alternatives {
        first: app.source(None),
        second: app.source(Some("B".into())),
        flag: app.source(true),
    };
    app.provide(choices);
    app.mount(ElseChain).unwrap();
    assert_eq!(app.text(), ["second:B"]);
    choices.first.set(Some("A".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["first:A"]);
    choices.second.set(Some("C".into()));
    choices.first.set(None);
    app.flush().unwrap();
    assert_eq!(app.text(), ["second:C"]);
}

#[derive(Clone, PartialEq)]
enum Event {
    Left(String),
    Right(String),
    Pair { left: i32, right: i32 },
    Code(u8),
}
#[derive(Clone, Copy)]
struct Events(Signal<Event>);
#[component]
fn OrPattern() -> View {
    let event = use_context::<Events>().0;
    view! {
        match event.get() {
            Event::Left(name) | Event::Right(name) => Stateful(value: name),
            _ => Text("other"),
        }
    }
}
#[test]
fn or_pattern_variants_share_one_arm_and_local_state() {
    let app = Engine::new();
    let event = app.source(Event::Left("left".into()));
    let captures = Captures::default();
    app.provide(Events(event));
    app.provide(captures.clone());
    app.mount(OrPattern).unwrap();
    captures.0.borrow()[0].set(6);
    event.set(Event::Right("right".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["right:6"]);
    assert_eq!(app.stats().mounts["Stateful"], 1);
}
#[component]
fn IfLetOr() -> View {
    let event = use_context::<Events>().0;
    view! {
        if let Event::Left(name) | Event::Right(name) = event.get() {
            Text(name)
        } else {
            Text("other")
        }
    }
}
#[test]
fn if_let_accepts_top_level_or_patterns() {
    let app = Engine::new();
    let event = app.source(Event::Left("left".into()));
    app.provide(Events(event));
    app.mount(IfLetOr).unwrap();
    event.set(Event::Right("right".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["right"]);
}
#[component]
fn AtPattern() -> View {
    let event = use_context::<Events>().0;
    view! {
        match event.get() {
            whole @ Event::Code(code @ (1 | 3..=7)) =>
                Text(format!("{}:{}", matches!(whole.get(), Event::Code(_)), code.get())),
            _ => Text("other"),
        }
    }
}
#[test]
fn owned_at_binding_nested_range_and_or_pattern_update_together() {
    let app = Engine::new();
    let event = app.source(Event::Code(1));
    app.provide(Events(event));
    app.mount(AtPattern).unwrap();
    assert_eq!(app.text(), ["true:1"]);
    event.set(Event::Code(5));
    app.flush().unwrap();
    assert_eq!(app.text(), ["true:5"]);
    event.set(Event::Code(2));
    app.flush().unwrap();
    assert_eq!(app.text(), ["other"]);
}
#[component]
fn IfLetAt() -> View {
    let event = use_context::<Events>().0;
    view! { if let whole @ Event::Code(code @ 3..=7) = event.get() {
        Text(format!("{}:{}", matches!(whole.get(), Event::Code(_)), code.get()))
    } }
}
#[test]
fn if_let_accepts_owned_at_binding() {
    let app = Engine::new();
    let event = app.source(Event::Code(3));
    app.provide(Events(event));
    app.mount(IfLetAt).unwrap();
    event.set(Event::Code(7));
    app.flush().unwrap();
    assert_eq!(app.text(), ["true:7"]);
}

#[derive(Clone, Default)]
struct Calls(Rc<RefCell<Vec<&'static str>>>);
impl Calls {
    fn source<T>(&self, value: T) -> T {
        self.0.borrow_mut().push("source");
        value
    }
    fn guard(&self, left: i32, right: i32) -> bool {
        self.0.borrow_mut().push("guard");
        right == left * 2
    }
}
#[derive(Clone, Default)]
struct Seen(Rc<RefCell<Vec<(i32, i32)>>>);
#[component]
fn PairView(left: ReadProp<i32>, right: ReadProp<i32>) -> View {
    let seen = use_context::<Seen>();
    let observed = use_memo(move || {
        let pair = (left.get(), right.get());
        seen.0.borrow_mut().push(pair);
        pair
    });
    view! { Text(format!("{:?}", observed.get())) }
}
#[component]
fn CountedMatch() -> View {
    let event = use_context::<Events>().0;
    let calls = use_context::<Calls>();
    view! {
        match calls.source(event.get()) {
            Event::Pair { left, right } if calls.guard(left, right) =>
                PairView(left: left, right: right),
            _ => Text("unmatched"),
        }
    }
}
#[test]
fn match_evaluates_source_and_guard_once_then_publishes_consistent_payload() {
    let app = Engine::new();
    let event = app.source(Event::Pair { left: 1, right: 2 });
    let calls = Calls::default();
    let seen = Seen::default();
    app.provide(Events(event));
    app.provide(calls.clone());
    app.provide(seen.clone());
    app.mount(CountedMatch).unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
    calls.0.borrow_mut().clear();
    event.set(Event::Pair { left: 3, right: 6 });
    app.flush().unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
    assert_eq!(*seen.0.borrow(), [(1, 2), (3, 6)]);
    assert_eq!(app.stats().mounts["PairView"], 1);
}

#[derive(Clone, Copy)]
struct Numbers(Signal<Option<(i32, i32)>>);
#[component]
fn ChainedPair() -> View {
    let numbers = use_context::<Numbers>().0;
    let calls = use_context::<Calls>();
    view! {
        if let Some((left, right)) = calls.source(numbers.get())
            && calls.guard(left, right)
        {
            PairView(left: left, right: right)
        }
    }
}
#[test]
fn if_let_chain_evaluates_once_and_updates_all_fields_before_child_observation() {
    let app = Engine::new();
    let numbers = app.source(Some((1, 2)));
    let calls = Calls::default();
    let seen = Seen::default();
    app.provide(Numbers(numbers));
    app.provide(calls.clone());
    app.provide(seen.clone());
    app.mount(ChainedPair).unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
    calls.0.borrow_mut().clear();
    numbers.set(Some((4, 8)));
    app.flush().unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
    assert_eq!(*seen.0.borrow(), [(1, 2), (4, 8)]);
    numbers.set(None);
    app.flush().unwrap();
    calls.0.borrow_mut().clear();
    numbers.set(Some((5, 10)));
    app.flush().unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
}

#[component]
fn CachedIfLet() -> View {
    let source = use_context::<Optional>().0;
    view! { KeepAlive(max_entries: 1) {
        if let Some(name) = source.get() { Stateful(value: name) }
    } }
}
#[test]
fn cached_if_let_resumes_latest_payload_with_previous_local_state() {
    let (app, source, captures) = optional();
    app.mount(CachedIfLet).unwrap();
    captures.0.borrow()[0].set(9);
    source.set(None);
    app.flush().unwrap();
    source.set(Some("latest".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["latest:9"]);
    assert_eq!(app.stats().mounts["Stateful"], 1);
}

#[derive(Clone, Copy)]
struct Values(Signal<Vec<i32>>);
#[component]
fn OwnedSlicePattern() -> View {
    let values = use_context::<Values>().0;
    view! {
        match values.get() {
            values if values.is_empty() => Text("empty"),
            values => Text(format!("{}:{}", values.get()[0], values.get().len())),
        }
    }
}
#[test]
fn whole_owned_vector_binding_retains_ownership() {
    let app = Engine::new();
    let values = app.source(vec![1, 2]);
    app.provide(Values(values));
    app.mount(OwnedSlicePattern).unwrap();
    values.set(vec![3, 4, 5]);
    app.flush().unwrap();
    assert_eq!(app.text(), ["3:3"]);
}

#[derive(Clone, Copy)]
struct Array(Signal<[i32; 3]>);
#[component]
fn ArrayPattern() -> View {
    let values = use_context::<Array>().0;
    view! {
        match values.get() {
            [first @ 1..=9, rest @ ..] => Text(format!("{}:{:?}", first.get(), rest.get())),
            _ => Text("other"),
        }
    }
}
#[test]
fn owned_array_and_rest_pattern_are_supported() {
    let app = Engine::new();
    let values = app.source([1, 2, 3]);
    app.provide(Array(values));
    app.mount(ArrayPattern).unwrap();
    values.set([4, 5, 6]);
    app.flush().unwrap();
    assert_eq!(app.text(), ["4:[5, 6]"]);
}

#[component]
fn GuardOrder() -> View {
    let event = use_context::<Events>().0;
    let calls = use_context::<Calls>();
    view! {
        match calls.source(event.get()) {
            Event::Pair { left, right } if calls.guard(left, right) => Text("first"),
            Event::Pair { left, right } if calls.guard(right, left) => Text("second"),
            _ => Text("fallback"),
        }
    }
}
#[test]
fn guard_order_and_first_success_follow_rust_match_semantics() {
    let app = Engine::new();
    let calls = Calls::default();
    let event = app.source(Event::Pair { left: 1, right: 2 });
    app.provide(Events(event));
    app.provide(calls.clone());
    app.mount(GuardOrder).unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
    assert_eq!(app.text(), ["first"]);
    calls.0.borrow_mut().clear();
    event.set(Event::Pair { left: 4, right: 2 });
    app.flush().unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard", "guard"]);
    assert_eq!(app.text(), ["second"]);
}

#[component]
fn CachedMatch() -> View {
    let event = use_context::<Events>().0;
    view! { KeepAlive(max_entries: 1) {
        match event.get() {
            Event::Left(name) | Event::Right(name) => Stateful(value: name),
            _ => Text("inactive"),
        }
    } }
}
#[test]
fn cached_match_resumes_payload_and_state_after_variant_changed_while_hidden() {
    let app = Engine::new();
    let event = app.source(Event::Left("A".into()));
    let captures = Captures::default();
    app.provide(Events(event));
    app.provide(captures.clone());
    app.mount(CachedMatch).unwrap();
    captures.0.borrow()[0].set(8);
    event.set(Event::Code(0));
    app.flush().unwrap();
    event.set(Event::Right("B".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["B:8"]);
    assert_eq!(app.stats().mounts["Stateful"], 1);
}

#[derive(Clone, Copy)]
struct Open(Signal<bool>);
#[component]
fn ConditionalFailure() -> View {
    let open = use_context::<Open>().0;
    view! {
        if open.get() {
            for row in vec![1, 1] key *row { Text(row) }
        } else {
            Text("committed")
        }
    }
}
#[test]
fn failed_new_branch_preparation_keeps_committed_branch_and_cleans_scratch_scope() {
    let app = Engine::new();
    let open = app.source(false);
    app.provide(Open(open));
    app.mount(ConditionalFailure).unwrap();
    let before = app.stats();
    open.set(true);
    assert!(matches!(app.flush(), Err(Error::DuplicateKey(_))));
    assert_eq!(app.text(), ["committed"]);
    assert_eq!(app.stats().live_scopes, before.live_scopes);
    assert_eq!(app.stats().live_signals, before.live_signals);
    assert_eq!(app.stats().live_watchers, before.live_watchers);
    open.set(false);
    app.flush().unwrap();
    assert_eq!(app.text(), ["committed"]);
}

#[test]
fn repeated_if_let_failure_and_reentry_do_not_grow_live_resources() {
    let (app, source, _) = optional();
    source.set(None);
    app.mount(IfLet).unwrap();
    let before = app.stats();
    for n in 0..200 {
        source.set(Some(format!("entry-{n}")));
        app.flush().unwrap();
        source.set(None);
        app.flush().unwrap();
    }
    assert_eq!(app.stats().live_scopes, before.live_scopes);
    assert_eq!(app.stats().live_signals, before.live_signals);
    assert_eq!(app.stats().live_watchers, before.live_watchers);
}

mod constants {
    pub const FIRST: u8 = 1;
}
#[component]
fn QualifiedConstant() -> View {
    let event = use_context::<Events>().0;
    view! {
        match event.get() {
            Event::Code(constants::FIRST) => Text("first"),
            Event::Code(code) => Text(code),
            _ => Text("other"),
        }
    }
}
#[test]
fn qualified_constants_remain_constant_patterns() {
    let app = Engine::new();
    let event = app.source(Event::Code(1));
    app.provide(Events(event));
    app.mount(QualifiedConstant).unwrap();
    assert_eq!(app.text(), ["first"]);
    event.set(Event::Code(2));
    app.flush().unwrap();
    assert_eq!(app.text(), ["2"]);
}

#[component]
fn ShadowChain() -> View {
    let source = use_context::<Optional>().0;
    view! {
        if let Some(value) = source.get()
            && let Some(value) = value.strip_prefix("ok:").map(str::to_owned)
        {
            Text(value)
        } else { Text("invalid") }
    }
}
#[test]
fn chain_shadowing_exports_the_final_binding() {
    let (app, source, _) = optional();
    source.set(Some("ok:A".into()));
    app.mount(ShadowChain).unwrap();
    assert_eq!(app.text(), ["A"]);
    source.set(Some("ok:B".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["B"]);
}

#[component]
fn OptionMatch() -> View {
    let source = use_context::<Optional>().0;
    view! { match source.get() {
        Some(name) => Text(name),
        None => Text("none"),
    } }
}
#[test]
fn bare_unit_variant_is_not_mistaken_for_payload() {
    let (app, source, _) = optional();
    app.mount(OptionMatch).unwrap();
    source.set(None);
    app.flush().unwrap();
    assert_eq!(app.text(), ["none"]);
    source.set(Some("again".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["again"]);
}

#[component]
fn BareConstant() -> View {
    use constants::FIRST;
    let event = use_context::<Events>().0;
    view! { match event.get() {
        Event::Code(FIRST) => Text("first"),
        _ => Text("other"),
    } }
}
#[test]
fn unreferenced_bare_constant_remains_a_pattern() {
    let app = Engine::new();
    let event = app.source(Event::Code(1));
    app.provide(Events(event));
    app.mount(BareConstant).unwrap();
    assert_eq!(app.text(), ["first"]);
    event.set(Event::Code(2));
    app.flush().unwrap();
    assert_eq!(app.text(), ["other"]);
}

#[component]
fn Hygiene() -> View {
    let source = use_context::<Optional>().0;
    view! { if let Some(__probe_payload) = source.get() {
        Text(__probe_payload)
    } }
}
#[test]
fn generated_payload_parameter_does_not_capture_user_identifier() {
    let (app, source, _) = optional();
    app.mount(Hygiene).unwrap();
    source.set(Some("hygienic".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["hygienic"]);
}

#[component]
fn RawIdentifier() -> View {
    let source = use_context::<Optional>().0;
    view! { if let Some(r#type) = source.get() { Text(r#type) } }
}
#[test]
fn raw_identifiers_remain_valid_reactive_bindings() {
    let (app, source, _) = optional();
    app.mount(RawIdentifier).unwrap();
    source.set(Some("raw".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["raw"]);
}

#[component]
fn AlternativeGuard() -> View {
    let source = use_context::<Numbers>().0;
    let calls = use_context::<Calls>();
    view! { match calls.source(source.get()) {
        Some((value, _)) | Some((_, value)) if calls.guard(value, 2) => Text(value),
        _ => Text("none"),
    } }
}
#[test]
fn overlapping_or_pattern_retries_guard_exactly_as_native_rust_does() {
    let app = Engine::new();
    let calls = Calls::default();
    let source = app.source(Some((9, 1)));
    app.provide(Numbers(source));
    app.provide(calls.clone());
    app.mount(AlternativeGuard).unwrap();
    assert_eq!(app.text(), ["1"]);
    assert_eq!(*calls.0.borrow(), ["source", "guard", "guard"]);
    calls.0.borrow_mut().clear();
    source.set(Some((1, 9)));
    app.flush().unwrap();
    assert_eq!(*calls.0.borrow(), ["source", "guard"]);
}

#[component]
fn CountedStateful() -> View {
    let source = use_context::<Optional>().0;
    let calls = use_context::<Calls>();
    view! { if let Some(name) = calls.source(source.get()) { Stateful(value: name) } }
}
#[test]
fn child_local_updates_do_not_resubscribe_or_rerun_parent_selection() {
    let (app, _, captures) = optional();
    let calls = Calls::default();
    app.provide(calls.clone());
    app.mount(CountedStateful).unwrap();
    calls.0.borrow_mut().clear();
    captures.0.borrow()[0].set(12);
    app.flush().unwrap();
    assert!(calls.0.borrow().is_empty());
    assert_eq!(app.text(), ["A:12"]);
}

#[component]
fn ChainAndKeyed() -> View {
    let source = use_context::<Optional>().0;
    let values = use_context::<Values>().0;
    view! { if let Some(prefix) = source.get() && !prefix.is_empty() {
        for row in values.get() key *row {
            Text(format!("{}:{}", prefix.get(), row.get()))
        }
    } }
}
#[test]
fn pattern_payload_updates_through_keyed_children() {
    let (app, source, _) = optional();
    let values = app.source(vec![1, 2]);
    app.provide(Values(values));
    app.mount(ChainAndKeyed).unwrap();
    source.set(Some("B".into()));
    values.set(vec![2, 1]);
    app.flush().unwrap();
    assert_eq!(app.text(), ["B:2", "B:1"]);
}

#[component]
fn NestedPatterns() -> View {
    let source = use_context::<Optional>().0;
    view! { if let Some(name) = source.get() {
        match name.get().len() {
            size @ 1..=3 => Text(format!("{}:{}", name.get(), size.get())),
            _ => Text("long"),
        }
    } }
}
#[test]
fn nested_match_sees_latest_if_let_payload_without_remounting_parent() {
    let (app, source, _) = optional();
    app.mount(NestedPatterns).unwrap();
    source.set(Some("BC".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["BC:2"]);
    source.set(Some("longer".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["long"]);
    assert_eq!(app.stats().mounts["NestedPatterns"], 1);
}

#[component]
fn BorrowAndCopy() -> View {
    let source = use_context::<Events>().0;
    view! { match &source.get() {
        &Event::Code(value) => Text(value),
        _ => Text("other"),
    } }
}
#[test]
fn explicit_reference_pattern_can_copy_an_owned_payload() {
    let app = Engine::new();
    let event = app.source(Event::Code(3));
    app.provide(Events(event));
    app.mount(BorrowAndCopy).unwrap();
    event.set(Event::Code(5));
    app.flush().unwrap();
    assert_eq!(app.text(), ["5"]);
}

#[component]
fn FormatCapture() -> View {
    let source = use_context::<Optional>().0;
    view! { if let Some(name) = source.get() { Text(format!("{name}")) } }
}
#[test]
fn implicit_format_capture_reads_current_payload() {
    let (app, source, _) = optional();
    app.mount(FormatCapture).unwrap();
    source.set(Some("updated".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["updated"]);
}

#[derive(Clone, Copy)]
struct OptionalNumbers(Signal<(Option<i32>, Option<i32>)>);
#[component]
fn MultiplePatternChain() -> View {
    let source = use_context::<OptionalNumbers>().0;
    view! { if let Some(left) = source.get().0 && left > 0
        && let Some(right) = source.get().1 && right > left
    { PairView(left: left, right: right) } }
}
#[test]
fn multiple_let_payloads_are_published_before_child_memo() {
    let app = Engine::new();
    let source = app.source((Some(1), Some(2)));
    let seen = Seen::default();
    app.provide(OptionalNumbers(source));
    app.provide(seen.clone());
    app.mount(MultiplePatternChain).unwrap();
    source.set((Some(3), Some(6)));
    app.flush().unwrap();
    assert_eq!(*seen.0.borrow(), [(1, 2), (3, 6)]);
}

#[test]
fn if_let_batched_failure_and_recovery_retains_committed_instance() {
    let (app, source, captures) = optional();
    app.mount(IfLet).unwrap();
    captures.0.borrow()[0].set(11);
    source.set(None);
    source.set(Some("B".into()));
    app.flush().unwrap();
    assert_eq!(app.text(), ["B:11"]);
    assert_eq!(app.stats().mounts["Stateful"], 1);
}

#[component]
fn PanickingBranch() -> View {
    let _scratch = use_state(|| 5);
    panic!("branch construction panic");
}
#[component]
fn PreparePanic() -> View {
    let open = use_context::<Open>().0;
    view! { if open.get() { PanickingBranch() } else { Text("old") } }
}
#[test]
fn panic_while_preparing_new_branch_cleans_scope_and_preserves_old_output() {
    let app = Engine::new();
    let open = app.source(false);
    app.provide(Open(open));
    app.mount(PreparePanic).unwrap();
    let before = app.stats();
    open.set(true);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.flush())).is_err());
    assert_eq!(app.text(), ["old"]);
    assert_eq!(app.stats().live_scopes, before.live_scopes);
    assert_eq!(app.stats().live_signals, before.live_signals);
    assert_eq!(app.stats().live_watchers, before.live_watchers);
    open.set(false);
    app.flush().unwrap();
    assert_eq!(app.text(), ["old"]);
}

#[derive(Clone, Copy)]
struct Many(Signal<[i32; 14]>);
#[component]
fn ManyBindings() -> View {
    let source = use_context::<Many>().0;
    view! { match source.get() {
        [a, b, c, d, e, f, g, h, i, j, k, l, m, n] =>
            Text(a.get()+b.get()+c.get()+d.get()+e.get()+f.get()+g.get()
                +h.get()+i.get()+j.get()+k.get()+l.get()+m.get()+n.get()),
    } }
}
#[test]
fn payload_has_no_flat_tuple_twelve_binding_limit() {
    let app = Engine::new();
    let source = app.source([1; 14]);
    app.provide(Many(source));
    app.mount(ManyBindings).unwrap();
    assert_eq!(app.text(), ["14"]);
    source.set([2; 14]);
    app.flush().unwrap();
    assert_eq!(app.text(), ["28"]);
    assert_eq!(app.stats().mounts["ManyBindings"], 1);
}
