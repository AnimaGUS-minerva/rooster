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

#[derive(StructOpt, PartialEq, Debug, Clone)]
/// Hermes Rooster Join-Proxy for ACP
pub struct RoosterOptions {
    /// turn on debugging from Grasp DULL
    #[structopt(long)]
    pub debug_graspmessages: bool,

    /// list of interfaces to ignore when auto-configuring
    #[structopt(long="--ignore-interface")]
    ignored_interfaces: Vec<String>,

    /// list of interfaces that should be uplink interfaces
    #[structopt(long="--acp-interface")]
    acp_interfaces: Vec<String>,

    /// list of interfaces that should be joinlink interfaces
    #[structopt(long="--joinlink-interface")]
    joinlink_interfaces: Vec<String>,
}

impl RoosterOptions {
    pub fn default() -> Self {
        RoosterOptions {
            debug_graspmessages: true,
            ignored_interfaces: vec![],
            acp_interfaces: vec![],
            downlink_interfaces: vec![]
        }
    }

    pub fn is_valid_acp_interface(self: &Self, ifname: &String) -> bool {
        if self.ignored_interfaces.contains(ifname) {
            return false;
        }

        // must explicitely mentioned, otherwise, it is a downlink interface
        if self.acp_interfaces.contains(ifname) {
            return true;
        }
        return false;
    }

    pub fn is_valid_downlink_interface(self: &Self, ifname: &String) -> bool {
        if self.ignored_interfaces.contains(ifname) {
            return false;
        }

        // if it is mentioned, then consider it spoken for
        if self.downlink_interfaces.contains(ifname) {
            return true;
        }

        // otherwise, if the list is empty, then it is automatically chosen
        if self.downlink_interfaces.is_empty() {
            return true;
        }
        return false;
    }
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
    fn test_acp_two_add() -> Result<(), std::io::Error> {
        assert_eq!(
            RoosterOptions::from_iter_safe(&["rooster",
                                             "--acp-interface=eth0",
                                             "--acp-interface=eth1"
            ]).unwrap(),
            RoosterOptions {
                debug_graspmessages: false,
                ignored_interfaces: vec![],
                acp_interfaces: vec!["eth0".to_string(),"eth1".to_string()],
                downlink_interfaces: vec![]
            });
        Ok(())
    }

    #[test]
    #[should_panic]
    fn test_help_msg() -> () {
        RoosterOptions::from_iter_safe(&["rooster","--help"]).unwrap();
        ()
    }

    #[test]
    fn test_eth0_is_ignored() -> Result<(), std::io::Error> {
        let ro1 = RoosterOptions {
            debug_graspmessages: false,
            ignored_interfaces: vec!["eth0".to_string()],
            acp_interfaces: vec![],
            downlink_interfaces: vec!["eth0".to_string()]
        };
        assert_eq!(ro1.is_valid_downlink_interface(&"eth0".to_string()), false);
        assert_eq!(ro1.is_valid_acp_interface(&"eth0".to_string()), false);
        assert_eq!(ro1.is_valid_downlink_interface(&"eth1".to_string()), false);
        Ok(())
    }

    #[test]
    fn test_eth0_is_downlink() -> Result<(), std::io::Error> {
        let ro1 = RoosterOptions {
            debug_graspmessages: false,
            ignored_interfaces: vec!["eth1".to_string()],
            acp_interfaces: vec![],
            downlink_interfaces: vec!["eth0".to_string()]
        };
        assert_eq!(ro1.is_valid_downlink_interface(&"eth0".to_string()), true);
        assert_eq!(ro1.is_valid_acp_interface(&"eth0".to_string()), false);
        Ok(())
    }

    #[test]
    fn test_eth0_is_acp() -> Result<(), std::io::Error> {
        let ro1 = RoosterOptions {
            debug_graspmessages: false,
            ignored_interfaces: vec!["eth1".to_string()],
            acp_interfaces: vec!["eth2".to_string()],
            downlink_interfaces: vec!["eth0".to_string()]
        };
        assert_eq!(ro1.is_valid_downlink_interface(&"eth2".to_string()), false);
        assert_eq!(ro1.is_valid_acp_interface(&"eth2".to_string()), true);
        Ok(())
    }

    #[test]
    fn test_eth0_is_implicit_downlink() -> Result<(), std::io::Error> {
        let ro1 = RoosterOptions {
            debug_graspmessages: false,
            ignored_interfaces: vec![],
            acp_interfaces: vec![],
            downlink_interfaces: vec![]
        };
        assert_eq!(ro1.is_valid_downlink_interface(&"eth2".to_string()), true);
        assert_eq!(ro1.is_valid_acp_interface(&"eth2".to_string()), false);
        Ok(())
    }
}


/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo build"
 * End:
 */
