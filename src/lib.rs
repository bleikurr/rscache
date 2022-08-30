use std::sync::RwLock;
use std::sync::Arc;
use std::time::Instant;

struct CacheData<T, S, F>
    where F: Fn(Option<&mut S>) -> T 
{
    data: Arc<T>,
    ttl: u128,
    state: Option<S>,
    age: std::time::Instant,
    refresher: F
}

impl<T, S, F> CacheData<T, S, F>
    where F: Fn(Option<&mut S>) -> T
{
    fn get_ref(&self) -> Result<Arc<T>, ()> {
        if self.age.elapsed().as_millis() > self.ttl {
            return Err(());
        }
        Ok(Arc::clone(&self.data))
    }

    fn refresh(&mut self) -> Arc<T> {
        if self.age.elapsed().as_millis() < self.ttl { 
            return Arc::clone(&self.data);
        }

        match self.state.as_mut() {
            Some(state) => { 
                self.data = Arc::new(
                    (self.refresher)(Some(state))
                );
            }
            None => {
                self.data = Arc::new((self.refresher)(None));
            }
        }
        self.age = Instant::now();
        Arc::clone(&self.data)
    }
}

pub struct Cache<T, S, F>
    where F: Fn(Option<&mut S>) -> T
{
    data: RwLock<CacheData<T, S, F>>
}

type ACache<T, S, F> = Arc<Cache<T, S, F>>;

impl<T, S, F> Cache<T, S, F>
    where F: Fn(Option<&mut S>) -> T
{
    pub fn new(mut state: Option<S>, ttl: u128, refresher: F) -> ACache<T, S, F> {
        let data = match state.as_mut() {
            Some(state) => refresher(Some(state)),
            None => refresher(None)
        };
        let cd = CacheData {
            data: Arc::new(data),
            age: Instant::now(),
            state,
            ttl,
            refresher
        };
        Arc::new(
            Self {
                data: RwLock::new(cd)
            }
        )
    }

    pub fn get_data(&self) -> Arc<T> {
        {
            let data = self.data.read().unwrap();
            if let Ok(data) = data.get_ref() {
                return data;
            }
        }
        // Refresh if data is old
        let mut data = self.data.write().unwrap();
        data.refresh()
    }

    pub fn clone(cache: &ACache<T, S, F>) -> ACache<T, S, F> {
        Arc::clone(cache)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn cache_test() {
        struct State {
            i: u32
        }
        let s = State { i: 0 }; 


        let cache: ACache<u32, State, _> = Cache::new(Some(s), 50, |s| {
            let s = s.unwrap();
            s.i += 1;
            s.i
        });
        
        let cache1 = Cache::clone(&cache);
        let t1 = thread::spawn(move || {
            let mut data = cache1.get_data();
            assert_eq!(*data, 1);
            thread::sleep(Duration::from_millis(10));
            data = cache1.get_data();
            assert_eq!(*data, 1);
            thread::sleep(Duration::from_millis(50));
            data = cache1.get_data();
            assert_eq!(*data, 2);
            thread::sleep(Duration::from_millis(20));
            data = cache1.get_data();
            assert_eq!(*data, 2);
            thread::sleep(Duration::from_millis(50));
            data = cache1.get_data();
            assert_eq!(*data, 3);
        });

        let cache2 = Cache::clone(&cache);
        let t2 = thread::spawn(move || {
            let mut data = cache2.get_data();
            assert_eq!(*data, 1);
            thread::sleep(Duration::from_millis(20));
            data = cache2.get_data();
            assert_eq!(*data, 1);
            thread::sleep(Duration::from_millis(40));
            data = cache2.get_data();
            assert_eq!(*data, 2);
            thread::sleep(Duration::from_millis(10));
            data = cache2.get_data();
            assert_eq!(*data, 2);
            thread::sleep(Duration::from_millis(50));
            data = cache2.get_data();
            assert_eq!(*data, 3);

        });


        t1.join().unwrap();
        t2.join().unwrap();
    }
}

