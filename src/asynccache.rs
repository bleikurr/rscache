use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::{Instant, Duration};

/// Result returned when fetching the cached data. Error returned is the error
/// returned from the "refresher function" supplied by the user.
pub type AsyncCacheResult<T> = Result<Arc<T>, Box<dyn std::error::Error + Send + Sync>>;

/// The type of result that is returned from the refresher function.
pub type AsyncRefreshResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;


// Trait used by encapsulated Cache structs.
trait DataCache {
    type Data;

    // Returns the data from the cache. Error if data is old. Error handled by cache.
    // If function returns Err it means the data is old.
    fn get_reference(&self) -> Result<Arc<Self::Data>, ()>;

    // Function that refreshes the data in the cache when old.
    fn refresh(&mut self) -> AsyncCacheResult<Self::Data>;
 }

// Implementation of DataCache encapsulated by AsyncCache.
// Used when data can be "statically" refreshed from a source.
// (Explained better in AsyncDataCacheWithState
// 
// T: Type of data being cached.
struct AsyncDataCache<T>
{
    data: Option<Arc<T>>,
    ttl: Duration,
    age: std::time::Instant,
    refresher: Box<dyn Fn() -> AsyncRefreshResult<T> + Send + Sync>
}

// Implementation of DataCache encapsulated in AsyncCache
// Takes a state that is used by the refresher function to dynamically
// update the data. F.x. if state is Arc<Mutex<S>> the state could be altered
// outside the cache in a safe way in async context (with the assumption 
// that the refresher function is done right). Thus affecting the data
// cached in a dynamic way.
// 
// T: Type of data being cached.
// S: State used by 'refresher'.
struct AsyncDataCacheWithState<T, S>
{
    data: Option<Arc<T>>,
    ttl: Duration,
    state: S,
    age: std::time::Instant,
    refresher: Box<dyn Fn(&mut S) -> AsyncRefreshResult<T> + Send + Sync>
}



impl<T, S> DataCache for AsyncDataCacheWithState<T, S>
{
    type Data = T;
    fn get_reference(&self) -> Result<Arc<Self::Data>, ()> {
        if self.age.elapsed() > self.ttl {
            return Err(());
        }

        Ok(Arc::clone(self.data.as_ref().unwrap()))
    }

    fn refresh(&mut self) -> AsyncCacheResult<Self::Data> {
        if self.age.elapsed() < self.ttl { 
            return Ok(Arc::clone(self.data.as_ref().unwrap()));
        }

        let data = (self.refresher)(&mut self.state);
        if data.is_ok() {
            self.data = Some(Arc::new(data.unwrap()));
            self.age = Instant::now();
            return Ok(Arc::clone(self.data.as_ref().unwrap()))
        }

        self.age = Instant::now();
        Err(data.err().unwrap())
    }
}

impl<T> DataCache for AsyncDataCache<T> 
{
    type Data = T; 
    fn get_reference(&self) -> Result<Arc<Self::Data>, ()> {
        if self.age.elapsed() > self.ttl {
            return Err(());
        }

        Ok(Arc::clone(self.data.as_ref().unwrap()))
    }

    fn refresh(&mut self) -> AsyncCacheResult<Self::Data> {
        if self.age.elapsed() < self.ttl { 
            return Ok(Arc::clone(self.data.as_ref().unwrap()));
        }
       
        let data = (self.refresher)();
        if data.is_ok() {
            self.data = Some(Arc::new(data.unwrap()));
            self.age = Instant::now();
            return Ok(Arc::clone(self.data.as_ref().unwrap()))
        }

        self.age = Instant::now();
        Err(data.err().unwrap())
    }

}

/// T: Type of data being cached.
/// Can be used with a State (see fn new_with_state)
/// 
/// Should be used with Arc to be shared in a multithreaded context.
/// ```
/// use std::sync::Arc;
/// use rusticache::AsyncCache;
/// use std::time::Duration;
///
/// async fn do_stuff() {
///     let cache = Arc::new(AsyncCache::new(
///         Duration::from_secs(10),
///         Box::new(|| Ok(String::from("This is Sparta!")))
///     ));
///
///     let data = cache.get_data().await;
///     assert_eq!(*data.unwrap(), String::from("This is Sparta!"));
/// }
/// do_stuff();
/// ```
pub struct AsyncCache<'a, T>
where T: 'a
{
    data: RwLock<Box<dyn DataCache<Data = T> + Send + Sync + 'a>>
}

impl<'a, T> AsyncCache<'a, T>
where T: Send + Sync + 'a
{
    /// Creates a new AsyncCache instance
    ///
    /// * `ttl` - Duration it takes for the date to get stale 
    /// * `refresher` - Closure that generates the data stored by the cache
    /// it is called internally by the cache when data grows stale.
    pub fn new(ttl: Duration, refresher: Box<dyn Fn() -> AsyncRefreshResult<T> + Send + Sync>)
        -> Self 
       {
        let datacache = AsyncDataCache {
            data: None,
            age: Instant::now() - ttl - ttl,
            ttl,
            refresher
        };
        AsyncCache {
            data: RwLock::new(Box::new(datacache) as Box<dyn DataCache<Data = T> + Send + Sync + 'a>)
        }
    }

    /// Creates a new AsyncCache instance with state
    ///
    /// * `state` - State used by internally by the cache when generating the data.
    /// It is passed to the refresher function when data is refreshed.
    /// * `ttl` - Duration it takes for the date to get stale 
    /// * `refresher` - Closure that generates the data stored by the cache
    /// it is called internally by the cache when data grows stale.
    ///
    /// ```
    /// use rusticache::AsyncCache;
    /// use std::time::Duration;
    ///
    /// async fn do_stuff() {
    ///     struct State {
    ///         i: u32
    ///     }
    ///     let s = State { i: 0 }; 
    ///
    ///     let cache = AsyncCache::new_with_state(s, Duration::from_millis(50), Box::new(|s| {
    ///         s.i = s.i + 1;
    ///         Ok(s.i)
    ///     }));
    ///
    ///     let data = cache.get_data().await;
    ///     assert_eq!(*data.unwrap(), 1);
    ///
    /// }
    ///
    /// do_stuff();
    /// ```
    pub fn new_with_state<S>(
        state: S,
        ttl: Duration,
        refresher: Box<dyn Fn(&mut S) -> AsyncRefreshResult<T> + Send + Sync >
    )  -> Self 
        where S: Send + Sync + 'a
    {
        let cd  = AsyncDataCacheWithState {
            data: None,
            age: Instant::now() - ttl - ttl,
            state,
            ttl,
            refresher
        };
        AsyncCache {
            data: RwLock::new(Box::new(cd) as Box<dyn DataCache<Data = T> + Send + Sync + 'a>)
        }
    }

    /// Returns readable data from the cache. Lazily refreshes data when stale.
    /// If a failure occurs it keeps old data and returns and error.
    pub async fn get_data(&self) -> AsyncCacheResult<T> {
        {
            let data = self.data.read().await;
            if let Ok(data) = data.get_reference() {
                return Ok(data);
            }
        }
        // Refresh if data is old
        let mut data = self.data.write().await;
        match data.refresh() {
            Ok(data) => Ok(data),
            Err(err) => Err(err)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread")]
    async fn cache_test() {
        struct State {
            i: u32
        }
        let s = State { i: 0 }; 


        let cache = Arc::new(AsyncCache::new_with_state(s, Duration::from_millis(50), Box::new(|s| {
            s.i = s.i + 1;
            Ok(s.i)
        })));
        
        let cache1 = Arc::clone(&cache);
        let t1 = tokio::spawn( async move {
            let mut data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(10)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(50)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(20)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(50)).await;
            data = cache1.get_data().await;
            assert_eq!(*data.unwrap(), 3);
        });

        let cache2 = Arc::clone(&cache);
        let t2 = tokio::spawn( async move {
            let mut data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(20)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 1);
            sleep(Duration::from_millis(40)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(10)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 2);
            sleep(Duration::from_millis(50)).await;
            data = cache2.get_data().await;
            assert_eq!(*data.unwrap(), 3);

        });


        t1.await.unwrap();
        t2.await.unwrap();
    }
}

