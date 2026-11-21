//! Vectors as the Nix derivations in `nix/` generate them: each source's JSON is turned into
//! binary twins, which a generated manifest reads with `include_bytes!`, so the suites need no
//! JSON or hex parser and run under Miri.

/// One example: its message and the intermediate items computed from it.
pub struct Vector<K: 'static> {
    pub name: &'static str,
    pub kind: K,
    pub output: &'static [u8],
    pub intermediates: &'static [Intermediate],
    pub external_aad: &'static [u8],
    pub unsent: &'static [Unsent],
    pub expect: Expect,
}

/// An intermediate item, at the layer its dotted path names (empty for the message itself).
pub struct Intermediate {
    pub at: &'static str,
    pub role: Role,
    pub bytes: &'static [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    ToBeSign,
    ToMac,
    Aad,
    Context,
}

/// The parts of a recipient's KDF context that are not sent in its headers.
pub struct Unsent {
    pub at: &'static str,
    pub apu_id: Option<&'static [u8]>,
    pub apv_id: Option<&'static [u8]>,
    pub pub_other: Option<&'static [u8]>,
    pub priv_other: Option<&'static [u8]>,
}

/// Whether a receiver accepts the example, and how its generator altered it.
pub enum Expect {
    Pass(&'static [Failure]),
    Fail(&'static [Failure]),
}

/// COSE-csharp's `ProcessFailures` operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    ChangeTag,
    ChangeAttr,
    AddProtected,
    RemoveProtected,
    ChangeProtected,
    RemoveCborTag,
    ChangeCborTag,
}

impl Expect {
    /// The generator's operations, whichever the outcome.
    pub const fn ops(&self) -> &'static [Failure] {
        match self {
            Self::Pass(ops) | Self::Fail(ops) => ops,
        }
    }
}

/// The message type an example's input names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sign,
    Sign1,
    Encrypt,
    Encrypt0,
    Mac,
    Mac0,
}

/// cose-wg/Examples (Unlicense), at the revision flake.lock pins.
pub static COSE_WG: [Vector<Kind>; 306] =
    include!(concat!(env!("COSE_WG_VECTORS"), "/manifest.rs"));

/// The example named `name`.
pub fn vector(name: &str) -> &'static Vector<Kind> {
    COSE_WG
        .iter()
        .find(|vector| vector.name == name)
        .unwrap_or_else(|| panic!("no example {name}"))
}

/// The example's intermediate at `at` in `role`.
pub fn intermediate(
    name: &str,
    at: &str,
    role: Role,
) -> (&'static Vector<Kind>, &'static Intermediate) {
    let vector = vector(name);
    vector
        .intermediates
        .iter()
        .find(|step| step.at == at && step.role == role)
        .map_or_else(
            || panic!("no {role:?} at {at:?} in {name}"),
            |step| (vector, step),
        )
}
