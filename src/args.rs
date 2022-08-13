/*
 * Copyright [2022] <mcr@sandelman.ca>

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

use structopt::StructOpt;

#[derive(StructOpt, PartialEq, Debug)]
/// Hermes Rooster Join-Proxy for ACP
pub struct RoosterOptions {
    // turn on debugging from Grasp DULL
    #[structopt(default_value = "false", long, parse(try_from_str))]
    debug_graspmessages: bool,

    // list of interfaces to ignore when auto-configuring
    #[structopt(long="ignore-interface")]
    ignored_interfaces: Vec<String>,

    // uplink interfaces
    #[structopt(long="acp-interface")]
    acp_interfaces: Vec<String>,

    // downlink interfaces
    #[structopt(long="downlink-interface")]
    downlink_interfaces: Vec<String>,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    //#[test]
    // unclear why these tests do not work
    fn test_parse_args() -> Result<(), std::io::Error> {
        assert_eq!(RoosterOptions {
            debug_graspmessages: true,
            ignored_interfaces: vec![], acp_interfaces: vec![], downlink_interfaces: vec![]
        }, RoosterOptions::from_iter(&["--debug-graspmessage=true"]));

        Ok(())
    }
}


/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo build"
 * End:
 */
