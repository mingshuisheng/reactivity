use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicU64;
use weak_table::WeakKeyHashMap;
use crate::reactive::{Reactive, ReactiveId, WeakReactiveId};

type DynFnType = Arc<dyn Fn() + Send + Sync>;

static EFFECT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct Effect {
    f: DynFnType,
    id: u64,
}

impl Effect {
    fn new(f: DynFnType) -> Self {
        Self {
            f,
            id: EFFECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        }
    }

    pub fn call(&self) {
        (self.f)();
    }
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Effect {}

impl Hash for Effect {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Debug for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Effect")
            .field("id", &self.id)
            .finish()
    }
}

static ID: AtomicU64 = AtomicU64::new(0);

pub struct Scope {
    parent: Option<Arc<Scope>>,
    functions: Arc<RwLock<Vec<Effect>>>,
    map: Arc<RwLock<WeakKeyHashMap<WeakReactiveId, HashSet<Effect>>>>,
    id: u64,
}

impl PartialEq for Scope {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Debug for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scope")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("functions", &self.functions.read().unwrap().len())
            .field("map", &self.map.read().unwrap().len())
            .finish()
    }
}

impl Scope {
    pub fn new() -> Arc<Self> {
        Arc::new(
            Self {
                parent: None,
                functions: Arc::new(RwLock::new(Vec::new())),
                map: Arc::new(RwLock::new(WeakKeyHashMap::new())),
                id: ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            }
        )
    }

    pub fn create_child(self: &Arc<Self>) -> Arc<Self> {
        Arc::new(
            Self {
                parent: Some(self.clone()),
                functions: Arc::new(RwLock::new(Vec::new())),
                map: Arc::new(RwLock::new(WeakKeyHashMap::new())),
                id: ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            }
        )
    }

    pub fn get_parent(self: &Arc<Self>) -> Option<Arc<Self>> {
        match &self.parent {
            Some(parent) => Some(parent.clone()),
            None => None,
        }
    }

    fn create_get_listen<T>(self: &mut Arc<Self>, id: ReactiveId) -> impl Fn(&T) + Send + 'static {
        let map = self.map.clone();
        let functions = self.functions.clone();
        let get_listen = move |_: &T| {
            if let Some(effect) = functions.read().unwrap().last() {
                if !map.read().unwrap().contains_key(&id) {
                    map.write().unwrap().insert(id.clone(), HashSet::new());
                }
                map.write().unwrap().get_mut(&id).unwrap().insert(effect.clone());
            }
        };

        get_listen
    }

    fn create_set_listen<T>(self: &mut Arc<Self>, id: ReactiveId) -> impl Fn(&T, &T) + Send + 'static {
        let map = self.map.clone();
        let set_listen = move |_: &T, _: &T| {
            if let Some(effects) = map.read().unwrap().get(&id) {
                effects.iter().for_each(|effect| effect.call());
            }
        };

        set_listen
    }

    pub fn reactive<T: Clone + Send + Sync>(self: &mut Arc<Self>, value: T) -> Reactive<T> {
        let reactive_value = Reactive::new(value);

        let get_listen = self.create_get_listen::<T>(reactive_value.id());
        let set_listen = self.create_set_listen::<T>(reactive_value.id());
        reactive_value.add_observer(get_listen, set_listen);

        reactive_value
    }

    pub fn effect(self: &mut Arc<Self>, f: impl Fn() + Send + Sync + 'static) {
        let effect = Effect::new(Arc::new(f));
        self.functions.write().unwrap().push(effect.clone());
        effect.call();
        self.functions.write().unwrap().pop();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_parent() {
        let scope = Scope::new();
        let child = scope.create_child();
        assert_eq!(child.get_parent().unwrap(), scope);
    }

    #[test]
    fn test_reactive() {
        let mut scope = Scope::new();
        let r = scope.reactive(0);
        let r2 = r.clone();
        let r3 = scope.reactive(0);
        dbg!(&scope);
        scope.effect(move || {
            println!("r2: {:?}", r2.value());
            let value = r2.value();
            r3.update(|_| value);
        });
        dbg!(&scope);
        let r4 = r.clone();
        scope.effect(move || {
            println!("r4: {:?}", r4.value());
        });
        dbg!(&scope);
        r.update(|count| count + 1);
        assert_eq!(r.value(), 1);
    }
}