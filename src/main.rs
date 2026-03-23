fn main() {
       #[cfg(not(target_arch="wasm32"))]
       {
           main::maverick_main()
       }
   }

