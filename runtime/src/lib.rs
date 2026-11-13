#![forbid(unsafe_code)]
#![doc = include_str!("../compile_fail.md")]
//! A headless feasibility probe, not the production UI runtime.
//! See README for the supported subset and explicit limitations.
pub use probe_macros::{component, view};
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fmt,
    marker::PhantomData,
    rc::{Rc, Weak},
};

type Id = u64;
pub type Factory = Rc<dyn Fn() -> View>;
type Work = Rc<dyn Fn() -> Result<(), Error>>;
type MountFn = Rc<dyn Fn(Host, Id) -> Result<Mounted, Error>>;
type KeyedEntries<K, T> = Rc<RefCell<BTreeMap<K, (Id, Signal<T>, Mounted)>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Disposed,
    Inactive,
    DuplicateKey(String),
    AlreadyMounted,
    NonConvergent,
    PayloadTypeMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignalLocation {
    pub runtime_id: u64,
    pub signal_id: u64,
    pub owner_scope: u64,
    pub owner_name: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherKind {
    Reactive,
    Layout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyLocation {
    pub watcher_id: u64,
    pub owner_scope: u64,
    pub owner_name: Option<&'static str>,
    pub kind: WatcherKind,
    pub layout_node: Option<LayoutNodeId>,
    pub label: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayoutNodeId(pub u64);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutResult {
    pub node: LayoutNodeId,
    pub label: &'static str,
    pub rect: Rect,
}

thread_local! {
    static NEXT: RefCell<Id> = const { RefCell::new(0) };
    static RUNTIMES: RefCell<HashMap<Id, Weak<RefCell<Inner>>>> = RefCell::new(HashMap::new());
    static CURRENT: RefCell<Option<Context>> = const { RefCell::new(None) };
    static TRACKING: RefCell<Vec<(Id, BTreeSet<Id>)>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct Host {
    id: Id,
    inner: Weak<RefCell<Inner>>,
}
impl Host {
    fn inner(&self) -> Rc<RefCell<Inner>> {
        self.inner.upgrade().expect("runtime has been dropped")
    }
}
#[derive(Clone)]
struct Context {
    host: Host,
    scope: Id,
    hooks: bool,
}
struct ContextGuard(Option<Context>);
impl Drop for ContextGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| *c.borrow_mut() = self.0.take());
    }
}
fn context<T>(host: &Host, scope: Id, hooks: bool, f: impl FnOnce() -> T) -> T {
    let previous = CURRENT.with(|c| {
        c.replace(Some(Context {
            host: host.clone(),
            scope,
            hooks,
        }))
    });
    let _guard = ContextGuard(previous);
    f()
}
fn current(hook: bool) -> Context {
    let context = CURRENT
        .with(|c| c.borrow().clone())
        .expect("no active component scope");
    assert!(
        !hook || context.hooks,
        "hooks are only valid during component initialization"
    );
    context
}
struct TrackingGuard;
impl Drop for TrackingGuard {
    fn drop(&mut self) {
        TRACKING.with(|t| {
            t.borrow_mut().pop();
        });
    }
}
fn evaluate<T>(host: &Host, scope: Id, f: impl FnOnce() -> T) -> (T, BTreeSet<Id>) {
    TRACKING.with(|t| t.borrow_mut().push((host.id, BTreeSet::new())));
    let guard = TrackingGuard;
    let value = context(host, scope, false, f);
    let deps = TRACKING.with(|t| t.borrow().last().unwrap().1.clone());
    drop(guard);
    (value, deps)
}

struct Scope {
    parent: Option<Id>,
    depth: usize,
    active: bool,
    enabled: bool,
    name: Option<&'static str>,
    children: BTreeSet<Id>,
    cleanups: Vec<Box<dyn FnOnce()>>,
}
struct Value {
    owner: Id,
    data: Box<dyn Any>,
}
struct Watcher {
    owner: Id,
    deps: BTreeSet<Id>,
    work: Work,
    kind: WatcherKind,
    layout_node: Option<LayoutNodeId>,
    label: Option<&'static str>,
}
#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub mounts: BTreeMap<String, usize>,
    pub disposals: BTreeMap<String, usize>,
    pub watcher_runs: usize,
    pub live_scopes: usize,
    pub live_signals: usize,
    pub live_watchers: usize,
    pub active_watchers: usize,
    pub signal_writes: usize,
    pub relayout_runs: usize,
    pub layout_nodes: usize,
}
struct Inner {
    next: Id,
    scopes: BTreeMap<Id, Scope>,
    values: BTreeMap<Id, Value>,
    watchers: BTreeMap<Id, Watcher>,
    dirty: BTreeSet<Id>,
    providers: HashMap<TypeId, Box<dyn Any>>,
    commands: HashMap<TypeId, Box<dyn Any>>,
    stats: Stats,
    root: Option<(Id, Mounted)>,
    layout: BTreeMap<LayoutNodeId, (Id, &'static str, Rc<RefCell<Rect>>)>,
}
impl Inner {
    fn id(&mut self) -> Id {
        self.next += 1;
        self.next
    }
}

pub struct Engine {
    id: Id,
    inner: Rc<RefCell<Inner>>,
}
impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
impl Engine {
    pub fn new() -> Self {
        let id = NEXT.with(|n| {
            *n.borrow_mut() += 1;
            *n.borrow()
        });
        let mut scopes = BTreeMap::new();
        scopes.insert(
            1,
            Scope {
                parent: None,
                depth: 0,
                active: true,
                enabled: true,
                name: None,
                children: BTreeSet::new(),
                cleanups: Vec::new(),
            },
        );
        let inner = Rc::new(RefCell::new(Inner {
            next: 1,
            scopes,
            values: BTreeMap::new(),
            watchers: BTreeMap::new(),
            dirty: BTreeSet::new(),
            providers: HashMap::new(),
            commands: HashMap::new(),
            stats: Stats::default(),
            root: None,
            layout: BTreeMap::new(),
        }));
        RUNTIMES.with(|r| r.borrow_mut().insert(id, Rc::downgrade(&inner)));
        Self { id, inner }
    }
    fn host(&self) -> Host {
        Host {
            id: self.id,
            inner: Rc::downgrade(&self.inner),
        }
    }
    pub fn source<T: Clone + PartialEq + 'static>(&self, value: T) -> Signal<T> {
        allocate(&self.host(), 1, value)
    }
    pub fn provide<T: Clone + 'static>(&self, value: T) {
        self.inner
            .borrow_mut()
            .providers
            .insert(TypeId::of::<T>(), Box::new(value));
    }
    pub fn store<K: StoreKey>(&self, value: K::State) -> StoreWriter<K::State> {
        let source = self.source(value);
        self.inner
            .borrow_mut()
            .providers
            .insert(TypeId::of::<K>(), Box::new(ReadStore(source.read_only())));
        StoreWriter(source)
    }
    pub fn command<C: Command>(
        &self,
        handler: impl Fn(C) -> Result<C::Output, C::Error> + 'static,
    ) {
        self.inner.borrow_mut().commands.insert(
            TypeId::of::<C>(),
            Box::new(CommandHandle::<C>(Rc::new(handler))),
        );
    }
    pub fn mount(&self, factory: impl FnOnce() -> View) -> Result<(), Error> {
        if self.inner.borrow().root.is_some() {
            return Err(Error::AlreadyMounted);
        }
        let host = self.host();
        let scope = new_scope(&host, 1, None);
        let mut rollback = BuildGuard::new(&host, scope);
        let view = context(&host, scope, false, factory);
        let mounted = mount(view, &host, scope)?;
        self.inner.borrow_mut().root = Some((scope, mounted));
        rollback.commit();
        self.flush()
    }
    pub fn flush(&self) -> Result<(), Error> {
        for _ in 0..10000 {
            let next = {
                let inner = self.inner.borrow();
                inner
                    .dirty
                    .iter()
                    .filter_map(|id| {
                        let w = inner.watchers.get(id)?;
                        let s = inner.scopes.get(&w.owner)?;
                        s.active.then_some((s.depth, *id))
                    })
                    .min()
            };
            let Some((_, id)) = next else {
                return Ok(());
            };
            self.inner.borrow_mut().dirty.remove(&id);
            run_watcher(&self.host(), id)?;
        }
        Err(Error::NonConvergent)
    }
    /// Re-run every active layout watcher. This is the explicit viewport or
    /// font-metric invalidation path when no reactive Signal changed.
    pub fn relayout(&self) -> Result<(), Error> {
        {
            let mut inner = self.inner.borrow_mut();
            let ids: Vec<_> = inner
                .watchers
                .iter()
                .filter(|(_, watcher)| {
                    watcher.kind == WatcherKind::Layout
                        && inner
                            .scopes
                            .get(&watcher.owner)
                            .is_some_and(|scope| scope.active)
                })
                .map(|(id, _)| *id)
                .collect();
            inner.dirty.extend(ids);
        }
        self.flush()
    }
    pub fn locate<T: Clone + PartialEq + 'static>(
        &self,
        signal: Signal<T>,
    ) -> Result<SignalLocation, Error> {
        signal.location()
    }
    pub fn dependents(&self, location: SignalLocation) -> Result<Vec<DependencyLocation>, Error> {
        if location.runtime_id != self.id {
            return Err(Error::Disposed);
        }
        let inner = self.inner.borrow();
        if !inner.values.contains_key(&location.signal_id) {
            return Err(Error::Disposed);
        }
        Ok(inner
            .watchers
            .iter()
            .filter(|(_, watcher)| watcher.deps.contains(&location.signal_id))
            .map(|(id, watcher)| DependencyLocation {
                watcher_id: *id,
                owner_scope: watcher.owner,
                owner_name: inner
                    .scopes
                    .get(&watcher.owner)
                    .and_then(|scope| scope.name),
                kind: watcher.kind,
                layout_node: watcher.layout_node,
                label: watcher.label,
            })
            .collect())
    }
    pub fn text(&self) -> Vec<String> {
        let mut result = Vec::new();
        if let Some((_, root)) = &self.inner.borrow().root {
            root.text(&mut result);
        }
        result
    }
    pub fn layout(&self) -> Vec<LayoutResult> {
        let inner = self.inner.borrow();
        inner
            .layout
            .iter()
            .map(|(node, (_, label, rect))| LayoutResult {
                node: *node,
                label,
                rect: *rect.borrow(),
            })
            .collect()
    }
    pub fn unmount(&self) {
        let root = self.inner.borrow_mut().root.take();
        if let Some((scope, _)) = root {
            dispose(&self.host(), scope);
        }
    }
    pub fn stats(&self) -> Stats {
        let inner = self.inner.borrow();
        let mut stats = inner.stats.clone();
        stats.live_scopes = inner.scopes.len();
        stats.live_signals = inner.values.len();
        stats.live_watchers = inner.watchers.len();
        stats.active_watchers = inner
            .watchers
            .values()
            .filter(|w| inner.scopes[&w.owner].active)
            .count();
        stats
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.unmount();
        RUNTIMES.with(|r| r.borrow_mut().remove(&self.id));
    }
}

pub struct Signal<T> {
    runtime: Id,
    id: Id,
    marker: PhantomData<(T, Rc<()>)>,
}
impl<T> Copy for Signal<T> {}
impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        (self.runtime, self.id) == (other.runtime, other.id)
    }
}
fn runtime(id: Id) -> Result<Rc<RefCell<Inner>>, Error> {
    RUNTIMES
        .with(|r| r.borrow().get(&id).and_then(Weak::upgrade))
        .ok_or(Error::Disposed)
}
impl<T: Clone + PartialEq + 'static> Signal<T> {
    pub fn location(self) -> Result<SignalLocation, Error> {
        let owner = runtime(self.runtime)?;
        let inner = owner.borrow();
        let value = inner.values.get(&self.id).ok_or(Error::Disposed)?;
        Ok(SignalLocation {
            runtime_id: self.runtime,
            signal_id: self.id,
            owner_scope: value.owner,
            owner_name: inner.scopes.get(&value.owner).and_then(|scope| scope.name),
        })
    }
    pub fn try_get(self) -> Result<T, Error> {
        let owner = runtime(self.runtime)?;
        let inner = owner.borrow();
        let value = inner.values.get(&self.id).ok_or(Error::Disposed)?;
        if !inner.scopes[&value.owner].active {
            return Err(Error::Inactive);
        }
        TRACKING.with(|t| {
            if let Some((runtime, deps)) = t.borrow_mut().last_mut() {
                assert_eq!(*runtime, self.runtime, "cross-runtime reactive dependency");
                deps.insert(self.id);
            }
        });
        Ok(value
            .data
            .downcast_ref::<T>()
            .expect("typed signal invariant")
            .clone())
    }
    pub fn get(self) -> T {
        self.try_get().expect("read of disposed/inactive state")
    }
    pub fn try_set(self, value: T) -> Result<bool, Error> {
        assert!(
            TRACKING.with(|t| t.borrow().is_empty()),
            "tracked expressions must not write state"
        );
        self.write(value)
    }
    pub fn set(self, value: T) {
        self.try_set(value)
            .expect("write to disposed/inactive state");
    }
    pub fn update(self, f: impl FnOnce(&mut T)) {
        let mut value = self.get();
        f(&mut value);
        self.set(value);
    }
    fn write(self, value: T) -> Result<bool, Error> {
        let owner = runtime(self.runtime)?;
        let mut inner = owner.borrow_mut();
        let state = inner.values.get(&self.id).ok_or(Error::Disposed)?;
        if !inner.scopes[&state.owner].active {
            return Err(Error::Inactive);
        }
        if state.data.downcast_ref::<T>() == Some(&value) {
            return Ok(false);
        }
        inner.values.get_mut(&self.id).unwrap().data = Box::new(value);
        inner.stats.signal_writes += 1;
        let affected: Vec<_> = inner
            .watchers
            .iter()
            .filter(|(_, w)| w.deps.contains(&self.id))
            .map(|(id, _)| *id)
            .collect();
        inner.dirty.extend(affected);
        Ok(true)
    }
    pub fn read_only(self) -> ReadProp<T> {
        ReadProp(self)
    }
}
pub struct ReadProp<T>(Signal<T>);
impl<T> Copy for ReadProp<T> {}
impl<T> Clone for ReadProp<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for ReadProp<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: Clone + PartialEq + 'static> ReadProp<T> {
    pub fn get(self) -> T {
        self.0.get()
    }
    pub fn project<U: Clone + PartialEq + 'static>(
        self,
        project: impl Fn(&T) -> U + 'static,
    ) -> ReadProp<U> {
        bind_prop(move || project(&self.get()))
    }
}
impl<T: Clone + PartialEq + fmt::Display + 'static> fmt::Display for ReadProp<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}
// The expected ReadProp<T> field type selects the value or existing binding impl.
pub trait PropValue<T> {
    fn read(self) -> T;
}
impl<T> PropValue<T> for T {
    fn read(self) -> T {
        self
    }
}
impl<T: Clone + PartialEq + 'static> PropValue<T> for ReadProp<T> {
    fn read(self) -> T {
        self.get()
    }
}

fn allocate<T: 'static>(host: &Host, scope: Id, value: T) -> Signal<T> {
    let owner = host.inner();
    let mut inner = owner.borrow_mut();
    let id = inner.id();
    inner.values.insert(
        id,
        Value {
            owner: scope,
            data: Box::new(value),
        },
    );
    Signal {
        runtime: host.id,
        id,
        marker: PhantomData,
    }
}
pub fn use_state<T: Clone + PartialEq + 'static>(initial: impl FnOnce() -> T) -> Signal<T> {
    let cx = current(true);
    allocate(&cx.host, cx.scope, initial())
}
pub fn bind_prop<T, V>(f: impl Fn() -> V + 'static) -> ReadProp<T>
where
    T: Clone + PartialEq + 'static,
    V: PropValue<T>,
{
    let cx = current(false);
    let (initial, deps) = evaluate(&cx.host, cx.scope, || f().read());
    let signal = allocate(&cx.host, cx.scope, initial);
    watch(
        &cx.host,
        cx.scope,
        deps,
        Rc::new(move || {
            signal.write(f().read())?;
            Ok(())
        }),
    );
    signal.read_only()
}
pub fn use_memo<T: Clone + PartialEq + 'static>(f: impl Fn() -> T + 'static) -> ReadProp<T> {
    current(true);
    bind_prop(f)
}
pub fn use_context<T: Clone + 'static>() -> T {
    let cx = current(true);
    let owner = cx.host.inner();
    let inner = owner.borrow();
    inner
        .providers
        .get(&TypeId::of::<T>())
        .unwrap_or_else(|| panic!("missing provider {}", std::any::type_name::<T>()))
        .downcast_ref::<T>()
        .unwrap()
        .clone()
}
pub fn on_dispose(cleanup: impl FnOnce() + 'static) {
    let cx = current(true);
    cx.host
        .inner()
        .borrow_mut()
        .scopes
        .get_mut(&cx.scope)
        .unwrap()
        .cleanups
        .push(Box::new(cleanup));
}
pub trait StoreKey: 'static {
    type State: Clone + PartialEq + 'static;
}
pub struct ReadStore<T>(ReadProp<T>);
impl<T> Copy for ReadStore<T> {}
impl<T> Clone for ReadStore<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Clone + PartialEq + 'static> ReadStore<T> {
    pub fn select<U: Clone + PartialEq + 'static>(
        self,
        f: impl Fn(&T) -> U + 'static,
    ) -> ReadProp<U> {
        bind_prop(move || f(&self.0.get()))
    }
}
pub struct StoreWriter<T>(Signal<T>);
impl<T> Copy for StoreWriter<T> {}
impl<T> Clone for StoreWriter<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Clone + PartialEq + 'static> StoreWriter<T> {
    pub fn update(self, f: impl FnOnce(&mut T)) {
        self.0.update(f);
    }
}
pub fn use_store<K: StoreKey>() -> ReadStore<K::State> {
    let cx = current(true);
    let owner = cx.host.inner();
    let inner = owner.borrow();
    *inner
        .providers
        .get(&TypeId::of::<K>())
        .expect("missing StoreKey provider")
        .downcast_ref::<ReadStore<K::State>>()
        .unwrap()
}
pub trait Command: 'static {
    type Output;
    type Error;
}
type Handler<C> = Rc<dyn Fn(C) -> Result<<C as Command>::Output, <C as Command>::Error>>;
pub struct CommandHandle<C: Command>(Handler<C>);
impl<C: Command> Clone for CommandHandle<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<C: Command> CommandHandle<C> {
    pub fn send(&self, command: C) -> Result<C::Output, C::Error> {
        assert!(
            TRACKING.with(|t| t.borrow().is_empty()),
            "commands are forbidden in tracked expressions"
        );
        (self.0)(command)
    }
}
pub fn use_command<C: Command>() -> CommandHandle<C> {
    let cx = current(true);
    let owner = cx.host.inner();
    let inner = owner.borrow();
    inner
        .commands
        .get(&TypeId::of::<C>())
        .expect("missing command provider")
        .downcast_ref::<CommandHandle<C>>()
        .unwrap()
        .clone()
}

fn new_scope(host: &Host, parent: Id, name: Option<&'static str>) -> Id {
    let owner = host.inner();
    let mut inner = owner.borrow_mut();
    let id = inner.id();
    let depth = inner.scopes[&parent].depth + 1;
    inner.scopes.get_mut(&parent).unwrap().children.insert(id);
    inner.scopes.insert(
        id,
        Scope {
            parent: Some(parent),
            depth,
            active: true,
            enabled: true,
            name,
            children: BTreeSet::new(),
            cleanups: Vec::new(),
        },
    );
    if let Some(name) = name {
        *inner.stats.mounts.entry(name.into()).or_default() += 1;
    }
    id
}
fn dispose(host: &Host, scope: Id) {
    let owner = host.inner();
    let removed = owner.borrow_mut().scopes.remove(&scope);
    if let Some(mut removed) = removed {
        for child in removed.children {
            dispose(host, child);
        }
        {
            let mut inner = owner.borrow_mut();
            if let Some(parent) = removed.parent.and_then(|id| inner.scopes.get_mut(&id)) {
                parent.children.remove(&scope);
            }
            inner.values.retain(|_, value| value.owner != scope);
            inner.layout.retain(|_, (owner, _, _)| *owner != scope);
            let watches: Vec<_> = inner
                .watchers
                .iter()
                .filter(|(_, w)| w.owner == scope)
                .map(|(id, _)| *id)
                .collect();
            for id in watches {
                inner.watchers.remove(&id);
                inner.dirty.remove(&id);
            }
            if let Some(name) = removed.name {
                *inner.stats.disposals.entry(name.into()).or_default() += 1;
            }
        }
        for cleanup in removed.cleanups.drain(..) {
            cleanup();
        }
    }
}
struct BuildGuard {
    host: Host,
    scope: Id,
    committed: bool,
}
impl BuildGuard {
    fn new(host: &Host, scope: Id) -> Self {
        Self {
            host: host.clone(),
            scope,
            committed: false,
        }
    }
    fn commit(&mut self) {
        self.committed = true;
    }
}
impl Drop for BuildGuard {
    fn drop(&mut self) {
        if !self.committed {
            dispose(&self.host, self.scope);
        }
    }
}
fn activate(host: &Host, scope: Id, active: bool) {
    host.inner()
        .borrow_mut()
        .scopes
        .get_mut(&scope)
        .unwrap()
        .enabled = active;
    let parent_active = {
        let owner = host.inner();
        let inner = owner.borrow();
        inner.scopes[&scope]
            .parent
            .is_none_or(|p| inner.scopes[&p].active)
    };
    refresh_active(host, scope, parent_active);
}
fn refresh_active(host: &Host, scope: Id, parent_active: bool) {
    let owner = host.inner();
    let (children, active) = {
        let mut inner = owner.borrow_mut();
        let s = inner.scopes.get_mut(&scope).unwrap();
        s.active = parent_active && s.enabled;
        (s.children.iter().copied().collect::<Vec<_>>(), s.active)
    };
    {
        let mut inner = owner.borrow_mut();
        let ids: Vec<_> = inner
            .watchers
            .iter()
            .filter(|(_, w)| w.owner == scope)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if active {
                inner.dirty.insert(id);
            } else {
                inner.watchers.get_mut(&id).unwrap().deps.clear();
                inner.dirty.remove(&id);
            }
        }
    }
    for child in children {
        refresh_active(host, child, active);
    }
}
fn watch(host: &Host, scope: Id, deps: BTreeSet<Id>, work: Work) -> Id {
    watch_with_meta(host, scope, deps, work, WatcherKind::Reactive, None, None)
}
fn watch_with_meta(
    host: &Host,
    scope: Id,
    deps: BTreeSet<Id>,
    work: Work,
    kind: WatcherKind,
    layout_node: Option<LayoutNodeId>,
    label: Option<&'static str>,
) -> Id {
    let owner = host.inner();
    let mut inner = owner.borrow_mut();
    let id = inner.id();
    inner.watchers.insert(
        id,
        Watcher {
            owner: scope,
            deps,
            work,
            kind,
            layout_node,
            label,
        },
    );
    id
}
fn run_watcher(host: &Host, id: Id) -> Result<(), Error> {
    let owner = host.inner();
    let (scope, work) = {
        let inner = owner.borrow();
        let Some(w) = inner.watchers.get(&id) else {
            return Ok(());
        };
        (w.owner, w.work.clone())
    };
    let (result, deps) = evaluate(host, scope, || work());
    let mut inner = owner.borrow_mut();
    inner.stats.watcher_runs += 1;
    if inner
        .watchers
        .get(&id)
        .is_some_and(|watcher| watcher.kind == WatcherKind::Layout)
    {
        inner.stats.relayout_runs += 1;
    }
    if let Some(w) = inner.watchers.get_mut(&id) {
        w.deps = deps;
    }
    result
}

pub struct View(Kind);
type PayloadUpdate = Rc<dyn Fn(Box<dyn Any>) -> Result<(), Error>>;
type PayloadBuild = Box<dyn FnOnce(&Host, Id, Box<dyn Any>) -> Result<BranchEntry, Error>>;

/// A single evaluated arm plus its complete, owned payload.
pub struct Selection {
    arm: usize,
    payload: Box<dyn Any>,
    build: PayloadBuild,
}
struct BranchEntry {
    arm: usize,
    scope: Id,
    node: Mounted,
    update: PayloadUpdate,
}
impl Selection {
    pub fn new<T: Clone + PartialEq + 'static>(
        arm: usize,
        payload: T,
        factory: impl FnOnce(ReadProp<T>) -> View + 'static,
    ) -> Self {
        Self {
            arm,
            payload: Box::new(payload),
            build: Box::new(move |host, scope, payload| {
                let value = payload
                    .downcast::<T>()
                    .map_err(|_| Error::PayloadTypeMismatch)?;
                let signal = allocate(host, scope, *value);
                // Construction reads must not subscribe the parent selector to
                // child-local state. Only the selector's own reads are tracked.
                let (node, _) = evaluate(host, scope, || {
                    mount(factory(signal.read_only()), host, scope)
                });
                Ok(BranchEntry {
                    arm,
                    scope,
                    node: node?,
                    update: Rc::new(move |payload| {
                        let value = payload
                            .downcast::<T>()
                            .map_err(|_| Error::PayloadTypeMismatch)?;
                        signal.write(*value)?;
                        Ok(())
                    }),
                })
            }),
        }
    }
}
enum Kind {
    Text(Rc<dyn Fn() -> String>),
    Group(Vec<View>),
    Component(&'static str, Factory),
    Switch {
        select: Rc<dyn Fn() -> Selection>,
        capacity: usize,
    },
    Custom(MountFn),
    Layout {
        label: &'static str,
        source: Rc<dyn Fn() -> Rect>,
    },
}
#[derive(Clone)]
enum Mounted {
    Text(Rc<RefCell<String>>),
    Group(Vec<Mounted>),
    Dynamic(Rc<RefCell<Vec<Mounted>>>),
    Layout { _node: LayoutNodeId, _host: Host },
}
impl Mounted {
    fn text(&self, output: &mut Vec<String>) {
        match self {
            Self::Text(text) => output.push(text.borrow().clone()),
            Self::Group(children) => {
                for child in children {
                    child.text(output);
                }
            }
            Self::Dynamic(children) => {
                for child in children.borrow().iter() {
                    child.text(output);
                }
            }
            Self::Layout { .. } => {}
        }
    }
}
impl View {
    pub fn text(f: impl Fn() -> String + 'static) -> Self {
        Self(Kind::Text(Rc::new(f)))
    }
    pub fn group(children: Vec<View>) -> Self {
        Self(Kind::Group(children))
    }
    pub fn component(name: &'static str, factory: impl Fn() -> View + 'static) -> Self {
        Self(Kind::Component(name, Rc::new(factory)))
    }
    pub fn layout(label: &'static str, source: impl Fn() -> Rect + 'static) -> Self {
        Self(Kind::Layout {
            label,
            source: Rc::new(source),
        })
    }
    pub fn select(select: impl Fn() -> Selection + 'static) -> Self {
        Self(Kind::Switch {
            select: Rc::new(select),
            capacity: 0,
        })
    }
    pub fn keep_alive(mut self, capacity: usize) -> Self {
        let Kind::Switch {
            capacity: limit, ..
        } = &mut self.0
        else {
            panic!("KeepAlive requires a switch")
        };
        *limit = capacity;
        self
    }
    pub fn keyed<T, K>(
        source: impl Fn() -> Vec<T> + 'static,
        key: impl Fn(&T) -> K + 'static,
        factory: impl Fn(ReadProp<T>) -> View + 'static,
    ) -> Self
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Ord + fmt::Debug + 'static,
    {
        let source = Rc::new(source);
        let key = Rc::new(key);
        let factory = Rc::new(factory);
        Self(Kind::Custom(Rc::new(move |host, parent| {
            let scope = new_scope(&host, parent, None);
            let output = Rc::new(RefCell::new(Vec::new()));
            let entries: KeyedEntries<K, T> = Rc::new(RefCell::new(BTreeMap::new()));
            let work: Work = {
                let (host, output, entries, source, key, factory) = (
                    host.clone(),
                    output.clone(),
                    entries.clone(),
                    source.clone(),
                    key.clone(),
                    factory.clone(),
                );
                Rc::new(move || {
                    let values = source();
                    let mut seen = BTreeSet::new();
                    let mut keyed = Vec::new();
                    // Validate the complete key set BEFORE changing any existing entry.
                    for value in values {
                        let k = key(&value);
                        if !seen.insert(k.clone()) {
                            return Err(Error::DuplicateKey(format!("{k:?}")));
                        }
                        keyed.push((k, value));
                    }
                    let old: Vec<_> = entries
                        .borrow()
                        .keys()
                        .filter(|k| !seen.contains(*k))
                        .cloned()
                        .collect();
                    for k in old {
                        let (scope, _, _) = entries.borrow_mut().remove(&k).unwrap();
                        dispose(&host, scope);
                    }
                    let mut ordered = Vec::new();
                    for (k, value) in keyed {
                        let existing = entries.borrow().get(&k).cloned();
                        if let Some((_, item, node)) = existing {
                            item.write(value)?;
                            ordered.push(node);
                        } else {
                            let child = new_scope(&host, scope, None);
                            let mut rollback = BuildGuard::new(&host, child);
                            let item = allocate(&host, child, value);
                            let node = context(&host, child, false, || {
                                mount(factory(item.read_only()), &host, child)
                            })?;
                            ordered.push(node.clone());
                            entries.borrow_mut().insert(k, (child, item, node));
                            rollback.commit();
                        }
                    }
                    *output.borrow_mut() = ordered;
                    Ok(())
                })
            };
            let id = watch(&host, scope, BTreeSet::new(), work);
            run_watcher(&host, id)?;
            Ok(Mounted::Dynamic(output))
        })))
    }
}

fn mount(view: View, host: &Host, parent: Id) -> Result<Mounted, Error> {
    match view.0 {
        Kind::Text(f) => {
            let (initial, deps) = evaluate(host, parent, || f());
            let text = Rc::new(RefCell::new(initial));
            let copy = text.clone();
            watch(
                host,
                parent,
                deps,
                Rc::new(move || {
                    *copy.borrow_mut() = f();
                    Ok(())
                }),
            );
            Ok(Mounted::Text(text))
        }
        Kind::Group(children) => Ok(Mounted::Group(
            children
                .into_iter()
                .map(|child| mount(child, host, parent))
                .collect::<Result<_, _>>()?,
        )),
        Kind::Component(name, factory) => {
            let scope = new_scope(host, parent, Some(name));
            let mut rollback = BuildGuard::new(host, scope);
            let view = context(host, scope, true, || factory());
            let result = mount(view, host, scope)?;
            rollback.commit();
            Ok(result)
        }
        Kind::Custom(mount) => mount(host.clone(), parent),
        Kind::Layout { label, source } => {
            let owner = host.inner();
            let node = {
                let mut inner = owner.borrow_mut();
                let node = LayoutNodeId(inner.id());
                inner.layout.insert(
                    node,
                    (parent, label, Rc::new(RefCell::new(Rect::default()))),
                );
                inner.stats.layout_nodes += 1;
                node
            };
            let (initial, deps) = evaluate(host, parent, || source());
            owner
                .borrow()
                .layout
                .get(&node)
                .expect("layout node invariant")
                .2
                .replace(initial);
            let rect = owner.borrow().layout[&node].2.clone();
            let source_for_watch = source.clone();
            watch_with_meta(
                host,
                parent,
                deps,
                Rc::new(move || {
                    rect.replace(source_for_watch());
                    Ok(())
                }),
                WatcherKind::Layout,
                Some(node),
                Some(label),
            );
            Ok(Mounted::Layout {
                _node: node,
                _host: host.clone(),
            })
        }
        Kind::Switch { select, capacity } => {
            let scope = new_scope(host, parent, None);
            let output = Rc::new(RefCell::new(Vec::new()));
            let active: Rc<RefCell<Option<BranchEntry>>> = Rc::new(RefCell::new(None));
            let cache: Rc<RefCell<VecDeque<BranchEntry>>> = Rc::new(RefCell::new(VecDeque::new()));
            let work: Work = {
                let (host, output) = (host.clone(), output.clone());
                Rc::new(move || {
                    let Selection {
                        arm,
                        payload,
                        build,
                    } = select();
                    if active
                        .borrow()
                        .as_ref()
                        .is_some_and(|entry| entry.arm == arm)
                    {
                        return (active.borrow().as_ref().unwrap().update)(payload);
                    }
                    let cached = {
                        let mut cache = cache.borrow_mut();
                        let index = cache.iter().position(|entry| entry.arm == arm);
                        index.and_then(|index| cache.remove(index))
                    };
                    // Prepare before retiring the committed branch. A failed
                    // fresh build leaves the old branch and its output alive.
                    let entry = if let Some(entry) = cached {
                        activate(&host, entry.scope, true);
                        (entry.update)(payload)?;
                        entry
                    } else {
                        let child = new_scope(&host, scope, None);
                        let mut rollback = BuildGuard::new(&host, child);
                        let entry = build(&host, child, payload)?;
                        rollback.commit();
                        entry
                    };
                    if let Some(previous) = active.borrow_mut().take() {
                        if capacity > 0 {
                            activate(&host, previous.scope, false);
                            cache.borrow_mut().push_back(previous);
                            while cache.borrow().len() > capacity {
                                let old = cache.borrow_mut().pop_front().unwrap();
                                dispose(&host, old.scope);
                            }
                        } else {
                            dispose(&host, previous.scope);
                        }
                    }
                    *output.borrow_mut() = vec![entry.node.clone()];
                    *active.borrow_mut() = Some(entry);
                    Ok(())
                })
            };
            let id = watch(host, scope, BTreeSet::new(), work);
            run_watcher(host, id)?;
            Ok(Mounted::Dynamic(output))
        }
    }
}

pub mod prelude {
    pub use crate::{
        Command, CommandHandle, DependencyLocation, Engine, Error, LayoutNodeId, LayoutResult,
        ReadProp, ReadStore, Rect, Signal, SignalLocation, StoreKey, StoreWriter, View,
        WatcherKind, bind_prop, component, on_dispose, use_command, use_context, use_memo,
        use_state, use_store, view,
    };
}
