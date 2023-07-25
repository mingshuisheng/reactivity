use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock, Weak};

pub type ReactiveId = Arc<u64>;
pub type WeakReactiveId = Weak<u64>;

static ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
pub struct Reactive<T> {
    inner: Arc<RwLock<ReactiveInner<T>>>,
    id: ReactiveId,
}

unsafe impl<T: Send> Send for Reactive<T> {}

unsafe impl<T: Sync> Sync for Reactive<T> {}

impl<T: Clone> Reactive<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ReactiveInner::new(value))),
            id: Arc::new(ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)),
        }
    }

    pub fn add_observer(&self, getter_observer: impl Fn(&T) + Send + 'static, setter_observer: impl Fn(&T, &T) + Send + 'static) {
        let mut inner = self.inner.write().unwrap();
        inner.getter_observers.push(Box::new(getter_observer));
        inner.setter_observers.push(Box::new(setter_observer));
    }

    pub fn id(&self) -> ReactiveId {
        self.id.clone()
    }
}

impl<T> PartialEq for Reactive<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Hash for Reactive<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> Eq for Reactive<T> {}

impl<T: Clone> Reactive<T> {
    pub fn value(&self) -> T {
        self.inner.read().unwrap().value()
    }
}

impl<T: PartialEq> Reactive<T> {
    pub fn update(&self, f: impl Fn(&T) -> T) {
        //inner的update方法返回的是Option<T>，如果new_value和old_value相等，返回None，否则返回Some(old_value)
        //并且需要及时释放写锁，否则会造成死锁
        let option = self.inner.write().unwrap().update(f);

        //重新获取读锁，遍历getter_observers
        if let Some(old_value) = option {
            self.inner.read().unwrap().traverse_setter_observers(&old_value);
        }
    }
}

impl<T: Debug> Debug for Reactive<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Reactive")
            .field(&self.inner.read().unwrap().value)
            .finish()
    }
}

#[derive(Default)]
pub struct ReactiveInner<T> {
    value: T,
    getter_observers: Vec<Box<dyn Fn(&T) + Send>>,
    setter_observers: Vec<Box<dyn Fn(&T, &T) + Send>>,
}

impl<T> ReactiveInner<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            getter_observers: Vec::new(),
            setter_observers: Vec::new(),
        }
    }

    pub fn traverse_getter_observers(&self) {
        let value = &self.value;
        for obs in &self.getter_observers {
            obs(value);
        }
    }

    pub fn traverse_setter_observers(&self, old_value: &T) {
        let value = &self.value;
        for obs in &self.setter_observers {
            obs(&value, &old_value);
        }
    }
}

impl<T: Clone> ReactiveInner<T> {
    pub fn value(&self) -> T {
        self.traverse_getter_observers();
        self.value.clone()
    }
}

impl<T: PartialEq> ReactiveInner<T> {
    pub fn update(&mut self, f: impl Fn(&T) -> T) -> Option<T> {
        let new_value = f(&self.value);
        if new_value != self.value {
            let old_value = std::mem::replace(&mut self.value, new_value);
            Some(old_value)
        } else {
            None
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_reactive() {
        let r = Reactive::new(0);
        r.add_observer(|val| println!("val: {}", val), |new_val, old_val| println!("new_val: {}, old_val: {}", new_val, old_val));
        r.update(|_| 1);
        assert_eq!(r.value(), 1);
    }

    #[test]
    fn test_reactive_vec() {
        let r = Reactive::new(vec![0]);
        r.add_observer(|val| println!("val: {:?}", val), |new_val, old_val| println!("new_val: {:?}, old_val: {:?}", new_val, old_val));
        r.update(|val| {
            let mut val = val.clone();
            val.push(1);
            val
        });
        assert_eq!(r.value(), vec![0, 1]);
    }
}