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
    #[structopt(long)]
    debug_graspmessages: bool,

    // list of interfaces to ignore when auto-configuring
    #[structopt(long="--ignore-interface")]
    ignored_interfaces: Vec<String>,

    // uplink interfaces
    #[structopt(long="--acp-interface")]
    acp_interfaces: Vec<String>,

    // downlink interfaces
    #[structopt(long="--downlink-interface")]
    downlink_interfaces: Vec<String>,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_parse_debugmsg() -> Result<(), std::io::Error> {
        assert_eq!(
            RoosterOptions::from_iter_safe(&["rooster","--debug-graspmessages"]).unwrap(),
            RoosterOptions {
                debug_graspmessages: true,
                ignored_interfaces: vec![], acp_interfaces: vec![],
                downlink_interfaces: vec![]
            });
        Ok(())
    }

    #[test]
    fn test_acp_add() -> Result<(), std::io::Error> {
        assert_eq!(
            RoosterOptions::from_iter_safe(&["rooster","--acp-interface=eth0"]).unwrap(),
            RoosterOptions {
                debug_graspmessages: false,
                ignored_interfaces: vec![], acp_interfaces: vec!["eth0".to_string()], downlink_interfaces: vec![]
            });
        Ok(())
    }

    #[test]
    #[should_panic]
    fn test_help_msg() -> () {
        RoosterOptions::from_iter_safe(&["rooster","--help"]).unwrap();
        ()
    }
}


/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo build"
 * End:
 */
