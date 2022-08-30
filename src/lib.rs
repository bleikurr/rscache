use std::sync::RwLock;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

struct CacheData<T, S>
{
    data: Arc<T>,
    ttl: u128,
    state: Option<S>,
    age: std::time::Instant,
    refresher: Box<dyn Fn(Option<&mut S>) -> T + Send + Sync>
}

impl<T, S> CacheData<T, S>
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

pub struct Cache<T, S>
{
    data: RwLock<CacheData<T, S>>
}

pub type ACache<T, S> = Arc<Cache<T, S>>;

impl<T, S> Cache<T, S>
{
    pub fn new(mut state: Option<S>, ttl: u128, refresher: Box<dyn Fn(Option<&mut S>) -> T + Send + Sync>) -> ACache<T, S> {
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
            std::thread::sleep(Duration::from_secs(1));
            let data = self.data.read().unwrap();
            if let Ok(data) = data.get_ref() {
                return data;
            }
        }
        // Refresh if data is old
        let mut data = self.data.write().unwrap();
        data.refresh()
    }

    pub fn clone(cache: &ACache<T, S>) -> ACache<T, S> {
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


        let cache: ACache<u32, State> = Cache::new(Some(s), 50, Box::new(|s| {
            let s = s.unwrap();
            s.i += 1;
            s.i
        }));
        
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

