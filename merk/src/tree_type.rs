use std::fmt;

#[cfg(feature = "minimal")]
use crate::merk::NodeType;
use crate::{Error, TreeFeatureType};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MaybeTree {
    Tree(TreeType),
    NotTree,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TreeType {
    NormalTree = 0,
    SumTree = 1,
    BigSumTree = 2,
    CountTree = 3,
    CountSumTree = 4,
    #[cfg(feature = "list_mode")]
    ListTree = 5, // For collaborative editing with positional operations
}

impl TryFrom<u8> for TreeType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TreeType::NormalTree),
            1 => Ok(TreeType::SumTree),
            2 => Ok(TreeType::BigSumTree),
            3 => Ok(TreeType::CountTree),
            4 => Ok(TreeType::CountSumTree),
            #[cfg(feature = "list_mode")]
            5 => Ok(TreeType::ListTree),
            #[cfg(feature = "list_mode")]
            n => Err(Error::UnknownTreeType(format!("got {}, max is 5", n))),
            #[cfg(not(feature = "list_mode"))]
            n => Err(Error::UnknownTreeType(format!("got {}, max is 4", n))),
        }
    }
}

impl fmt::Display for TreeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            TreeType::NormalTree => "Normal Tree",
            TreeType::SumTree => "Sum Tree",
            TreeType::BigSumTree => "Big Sum Tree",
            TreeType::CountTree => "Count Tree",
            TreeType::CountSumTree => "Count Sum Tree",
            #[cfg(feature = "list_mode")]
            TreeType::ListTree => "List Tree",
        };
        write!(f, "{}", s)
    }
}

impl TreeType {
    pub fn allows_sum_item(&self) -> bool {
        match self {
            TreeType::NormalTree => false,
            TreeType::SumTree => true,
            TreeType::BigSumTree => true,
            TreeType::CountTree => false,
            TreeType::CountSumTree => true,
            #[cfg(feature = "list_mode")]
            TreeType::ListTree => false,
        }
    }

    #[cfg(feature = "minimal")]
    pub const fn inner_node_type(&self) -> NodeType {
        match self {
            TreeType::NormalTree => NodeType::NormalNode,
            TreeType::SumTree => NodeType::SumNode,
            TreeType::BigSumTree => NodeType::BigSumNode,
            TreeType::CountTree => NodeType::CountNode,
            TreeType::CountSumTree => NodeType::CountSumNode,
            #[cfg(feature = "list_mode")]
            TreeType::ListTree => NodeType::NormalNode, // List trees use normal nodes with list_mode flag
        }
    }

    pub fn empty_tree_feature_type(&self) -> TreeFeatureType {
        match self {
            TreeType::NormalTree => TreeFeatureType::BasicMerkNode,
            TreeType::SumTree => TreeFeatureType::SummedMerkNode(0),
            TreeType::BigSumTree => TreeFeatureType::BigSummedMerkNode(0),
            TreeType::CountTree => TreeFeatureType::CountedMerkNode(0),
            TreeType::CountSumTree => TreeFeatureType::CountedSummedMerkNode(0, 0),
            #[cfg(feature = "list_mode")]
            TreeType::ListTree => TreeFeatureType::BasicMerkNode,
        }
    }
}
