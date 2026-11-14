#![forbid(unsafe_code)]
//! The map key order a type's encoding takes (D-11b), which a deterministic type checks each of its
//! fields against, so that every map under it is sorted the same way.

/// What a type's encoding commits to about map key order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    /// It writes no map: any order holds.
    Any,
    /// Declaration order, the default for a type that writes a map of its own.
    Declared,
    /// RFC 8949 §4.2.1: bytewise by the keys' encodings.
    Bytewise,
    /// RFC 8949 §4.2.3: shorter encodings first, then bytewise.
    LengthFirst,
    /// Parts that commit to two different orders.
    Conflict,
}

impl Order {
    /// The order of a value made of parts in `self` and `other`.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Any, order) | (order, Self::Any) => order,
            (Self::Declared, Self::Declared) => Self::Declared,
            (Self::Bytewise, Self::Bytewise) => Self::Bytewise,
            (Self::LengthFirst, Self::LengthFirst) => Self::LengthFirst,
            _ => Self::Conflict,
        }
    }

    /// Whether a type in `self` may hold a part in `part`: always, unless `self` sorts its keys, in
    /// which case every part must sort its keys the same way or write no map.
    #[must_use]
    pub const fn admits(self, part: Self) -> bool {
        matches!(
            (self, part),
            (Self::Any | Self::Declared | Self::Conflict, _)
                | (_, Self::Any)
                | (Self::Bytewise, Self::Bytewise)
                | (Self::LengthFirst, Self::LengthFirst)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Order;

    const ALL: [Order; 5] = [
        Order::Any,
        Order::Declared,
        Order::Bytewise,
        Order::LengthFirst,
        Order::Conflict,
    ];

    #[test]
    fn any_joins_as_the_other_and_two_orders_conflict() {
        for order in ALL {
            assert_eq!(Order::Any.join(order), order);
            assert_eq!(order.join(Order::Any), order);
            assert_eq!(order.join(Order::Conflict), Order::Conflict);
        }
        assert_eq!(Order::Declared.join(Order::Bytewise), Order::Conflict);
        assert_eq!(Order::Bytewise.join(Order::LengthFirst), Order::Conflict);
        assert_eq!(
            Order::LengthFirst.join(Order::LengthFirst),
            Order::LengthFirst
        );
    }

    #[test]
    fn a_sorting_order_admits_only_itself_and_any() {
        for sorting in [Order::Bytewise, Order::LengthFirst] {
            for part in ALL {
                assert_eq!(
                    sorting.admits(part),
                    part == Order::Any || part == sorting,
                    "{sorting:?} holding {part:?}"
                );
            }
        }
        for unsorted in [Order::Any, Order::Declared] {
            assert!(ALL.into_iter().all(|part| unsorted.admits(part)));
        }
    }
}
