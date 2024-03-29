/*
 * Copyright [2020,2023] <mcr@sandelman.ca>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
 *
 */

use std::io::Write;
use std::io;
use std::sync::Arc;
use futures::lock::Mutex;

#[derive(Clone, Debug)]
pub struct DebugOptions {
    pub debug_interfaces:      bool,       // interfaces added/removed
    pub debug_registrars:      bool,       // registrars announces
    pub debug_joininterfaces:  bool,       // join interface announcements
    pub debug_proxyactions:    bool,       // proxy connections and activity
    pub debug_output:      Arc<Mutex<dyn Write + Send>>
}

impl DebugOptions {
    pub fn default() -> DebugOptions {
        DebugOptions {
            debug_interfaces:  false,
            debug_registrars:  false,
            debug_joininterfaces:  false,
            debug_proxyactions:    false,
            debug_output:      Arc::new(Mutex::new(io::stdout()))
        }
    }

    pub async fn debug_interfaces(self: &Self,
                               msg: String) {
        if self.debug_interfaces {
            let mut output = self.debug_output.lock().await;
            writeln!(output, "I: {}", msg).unwrap();
        }
    }

    pub async fn debug_interfaces_detailed(self: &Self,
                                           msg: String) {
        if self.debug_interfaces {
            let mut output = self.debug_output.lock().await;
            writeln!(output, "I: {}", msg).unwrap();
        }
    }

    pub async fn debug_registrars(self: &Self,
                               msg: String) {
        if self.debug_registrars {
            let mut output = self.debug_output.lock().await;
            writeln!(output, "R: {}", msg).unwrap();
        }
    }

    pub async fn debug_registrars_detailed(self: &Self,
                                           msg: String) {
        if self.debug_registrars {
            let mut output = self.debug_output.lock().await;
            writeln!(output, "R: {}", msg).unwrap();
        }
    }

    pub async fn debug_joininterfaces(self: &Self,
                                      msg: String) {
        if self.debug_joininterfaces {
            let mut output = self.debug_output.lock().await;
            writeln!(output, "J: {}", msg).unwrap();
        }
    }

    pub async fn debug_proxyactions(self: &Self,
                                    msg: String) {
        if self.debug_proxyactions {
            let mut output = self.debug_output.lock().await;
            writeln!(output, "P: {}", msg).unwrap();
        }
    }

    pub async fn debug_info(self: &Self,
                            msg: String) {
        let mut output = self.debug_output.lock().await;
        writeln!(output, "I: {}", msg).unwrap();
    }

    pub async fn debug_error(self: &Self,
                             msg: String) {
        let mut output = self.debug_output.lock().await;
        writeln!(output, "E: {}", msg).unwrap();
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    async fn atest_debug_info(awriter: Arc<Mutex<Vec<u8>>>, db1: DebugOptions) -> Result<(), std::io::Error>  {
        db1.debug_info("hello".to_string()).await;
        let output = awriter.lock().await;
        let stuff = std::str::from_utf8(&output).unwrap();
        assert_eq!(stuff, "I: hello\n");
        Ok(())
    }

    #[test]
    fn test_debug_info() -> Result<(), std::io::Error> {
        let writer: Vec<u8> = vec![];
        let awriter = Arc::new(Mutex::new(writer));
        let db1 = DebugOptions { debug_interfaces: true,
                                 debug_registrars:  false,
                                 debug_joininterfaces:  false,
                                 debug_proxyactions:    false,
                                 debug_output: awriter.clone() };

        aw!(atest_debug_info(awriter.clone(), db1)).unwrap();
        Ok(())
    }
}


/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo test"
 * End:
 */
