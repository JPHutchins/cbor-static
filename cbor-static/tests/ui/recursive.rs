#[derive(cbor_static::Cbor)]
struct Node<'a> {
    value: u32,
    children: cbor_static::ArrayRef<'a, Node<'a>>,
}

#[derive(cbor_static::Cbor)]
enum Tree {
    Leaf(u32),
    Branch { left: Option<Self> },
}

fn main() {}
