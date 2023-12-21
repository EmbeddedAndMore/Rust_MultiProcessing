pub mod datev1{
    use std::cell::RefCell;
    use std::time::SystemTime;
    use httpdate::HttpDate;



    #[derive(Default)]
    pub struct DTCache{
        dt_s: String,
        unix_dt: u64
    }
    thread_local! {
        static NOW: RefCell<DTCache> = RefCell::new(DTCache{
            dt_s:"".to_string(),
            unix_dt:0
        })
    }

    impl DTCache {

        fn update(&mut self, str_dt: String, unix_dt:u64) {
            self.dt_s = str_dt;
            self.unix_dt = unix_dt;
        }
    }


    pub fn now1() ->String{
        let now = SystemTime::now();
        let now_unix  = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|epoch|{epoch.as_secs()})
            .unwrap();
        NOW.with(|cached_dt|{
            let mut cache = cached_dt.borrow_mut();
            if now_unix != cache.unix_dt{
                cache.update(HttpDate::from(now).to_string(), now_unix);

            }
        });
        return NOW.with(|cached|{cached.borrow().dt_s.clone()});
    }

}