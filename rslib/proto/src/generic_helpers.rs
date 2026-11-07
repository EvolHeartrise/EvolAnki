// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

impl From<String> for crate::generic::String {
    fn from(val: String) -> Self {
        crate::generic::String { val }
    }
}

impl From<Vec<String>> for crate::generic::StringList {
    fn from(vals: Vec<String>) -> Self {
        crate::generic::StringList { vals }
    }
}

impl From<bool> for crate::generic::Bool {
    fn from(val: bool) -> Self {
        crate::generic::Bool { val }
    }
}

impl From<u32> for crate::generic::UInt32 {
    fn from(val: u32) -> Self {
        crate::generic::UInt32 { val }
    }
}

impl From<usize> for crate::generic::UInt32 {
    fn from(val: usize) -> Self {
        crate::generic::UInt32 { val: val as u32 }
    }
}
