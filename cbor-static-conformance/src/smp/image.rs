use cbor_static::Cbor;
use heapless::Vec;

/// The most images a Zephyr build updates: the top of `CONFIG_MCUMGR_GRP_IMG_UPDATABLE_IMAGE_NUMBER`'s
/// range (`subsys/mgmt/mcumgr/grp/img_mgmt/Kconfig:47-50`), so any build's responses fit.
pub(crate) const UPDATABLE_IMAGE_NUMBER: usize = 3;

/// The slots of one image, a primary and a secondary, as `img_mgmt.c:129-133` counts them outside
/// the firmware-updater mode.
pub(crate) const SLOTS_PER_IMAGE: usize = 2;

/// State of images' response (command 0), to a read or a write: each image slot's state, and the
/// split status. Zephyr lists two slots for each image (`img_mgmt_state.c:560-561`).
///
/// ```
/// use cbor_static_conformance::smp::image::ImageStatesReadResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_states_read_response/1.cbor"));
/// let response: ImageStatesReadResponse = cbor_static::from_slice(written)?;
/// assert_eq!((response.images.len(), response.split_status), (2, Some(0)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageStatesReadResponse<'a> {
    #[serde(borrow)]
    pub images: Vec<ImageState<'a>, { SLOTS_PER_IMAGE * UPDATABLE_IMAGE_NUMBER }>,
    #[serde(rename = "splitStatus")]
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub split_status: Option<i64>,
}

/// One image slot's state. Zephyr writes every flag, or, under
/// `CONFIG_MCUMGR_GRP_IMG_FRUGAL_LIST`, only those that are true (`img_mgmt_state.c:37-46`). Keys
/// an application adds through `CONFIG_MCUMGR_GRP_IMG_IMAGE_SLOT_STATE_HOOK`
/// (`img_mgmt_state.c:537-541`) are out of scope, and decode as `UnknownField`.
///
/// ```
/// use cbor_static_conformance::smp::image::{ImageState, ImageStatesReadResponse};
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_states_read_response/1.cbor"));
/// let response: ImageStatesReadResponse = cbor_static::from_slice(written)?;
/// let ImageState { version, hash, active, .. } = response.images[0];
/// assert_eq!((version, hash, active), ("0.1.0", Some(b"A".as_slice()), Some(true)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageState<'a> {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub image: Option<i64>,
    pub slot: i64,
    #[serde(borrow)]
    pub version: &'a str,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub hash: Option<&'a [u8]>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub bootable: Option<bool>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub permanent: Option<bool>,
}

/// Set state of image (command 0): the image to test, or to confirm.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageStatesWriteRequest;
///
/// let built = include_bytes!(concat!(env!("SMP_VECTORS"), "/hand_built/image_states_write_request.cbor"));
/// let request: ImageStatesWriteRequest = cbor_static::from_slice(built)?;
/// assert_eq!((request.hash.map(<[u8]>::len), request.confirm), (Some(32), Some(true)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageStatesWriteRequest<'a> {
    #[cbor(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub hash: Option<&'a [u8]>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<bool>,
}

/// Image upload (command 1): one chunk of an image, at its offset. The first chunk also carries
/// the image's length and hash.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageUploadWriteRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_upload_write_request/1.cbor"));
/// let request: ImageUploadWriteRequest = cbor_static::from_slice(written)?;
/// assert_eq!((request.off, request.data, request.len), (0, b"hello".as_slice(), Some(5)));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageUploadWriteRequest<'a> {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub image: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub len: Option<i64>,
    pub off: i64,
    #[cbor(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub sha: Option<&'a [u8]>,
    #[serde(borrow)]
    pub data: &'a [u8],
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<bool>,
}

/// Image upload's response: the offset to send next, whether the image matched its hash, and an
/// error code.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageUploadWriteResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_upload_write_response/4.cbor"));
/// assert_eq!(cbor_static::from_slice::<ImageUploadWriteResponse>(written)?.off, Some(0));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageUploadWriteResponse {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub off: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub r#match: Option<bool>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub rc: Option<i64>,
}

/// Image erase (command 5): the slot to erase.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageEraseRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_erase_request/4.cbor"));
/// assert_eq!(cbor_static::from_slice::<ImageEraseRequest>(written)?.slot, Some(0));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageEraseRequest {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub slot: Option<i64>,
}

/// Slot info's response (command 6): each image's slots and their sizes, one entry for each
/// image (`img_mgmt.c:530-533`).
///
/// ```
/// use cbor_static_conformance::smp::image::ImageSlotInfoReadResponse;
///
/// let built = include_bytes!(concat!(env!("SMP_VECTORS"), "/hand_built/image_slot_info_read_response.cbor"));
/// let response: ImageSlotInfoReadResponse = cbor_static::from_slice(built)?;
/// assert_eq!(response.images.iter().map(|image| image.slots.len()).sum::<usize>(), 4);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageSlotInfoReadResponse {
    pub images: Vec<ImageSlotInfo, UPDATABLE_IMAGE_NUMBER>,
}

/// One image's slots, and the largest image it can take.
///
/// ```
/// use cbor_static_conformance::smp::image::{ImageSlotInfo, ImageSlotInfoReadResponse};
///
/// let built = include_bytes!(concat!(env!("SMP_VECTORS"), "/hand_built/image_slot_info_read_response.cbor"));
/// let response: ImageSlotInfoReadResponse = cbor_static::from_slice(built)?;
/// let [with, without] = [&response.images[0], &response.images[1]].map(|image| image.max_image_size);
/// assert!(with.is_some() && without.is_none());
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageSlotInfo {
    pub image: i64,
    pub slots: Vec<ImageSlot, SLOTS_PER_IMAGE>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub max_image_size: Option<i64>,
}

/// One slot of an image: its size, and the image ID that uploads to it directly. For a slot that
/// failed to open, Zephyr writes `flash_area_open`'s errno as `rc` in place of the size, inside
/// an otherwise successful response (`img_mgmt.c:558-565`); this is not SMP's error response,
/// which replaces the whole body. Zephyr's docs do not describe it
/// (zephyrproject-rtos/zephyr#126660), and smp's class does not read it (JPHutchins/smp#80). Keys
/// an application adds through `CONFIG_MCUMGR_GRP_IMG_SLOT_INFO_HOOKS` (`img_mgmt.c:609-613`) are
/// out of scope, and decode as `UnknownField`.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageSlot;
///
/// let failed = ImageSlot { slot: 1, size: None, rc: Some(-2), upload_image_id: None };
/// let mut out = [0u8; 16];
/// let len = cbor_static::to_slice(&failed, &mut out)?;
/// assert_eq!(&out[..len], b"\xa2\x62rc\x21\x64slot\x01");
/// assert_eq!(cbor_static::from_slice::<ImageSlot>(&out[..len])?, failed);
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageSlot {
    pub slot: i64,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub rc: Option<i64>,
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub upload_image_id: Option<i64>,
}

/// State of images (command 0), read: no fields.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageStatesReadRequest;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_states_read_request/1.cbor"));
/// cbor_static::from_slice::<ImageStatesReadRequest>(written)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type ImageStatesReadRequest = super::Empty;

/// Image erase's response: no fields, or, under `CONFIG_MCUMGR_SMP_LEGACY_RC_BEHAVIOUR` and in
/// every Zephyr before v3.3, an `rc` of 0 (`img_mgmt.c:497-502`), which smp's class does not read
/// (JPHutchins/smp#81).
///
/// ```
/// use cbor_static_conformance::smp::image::ImageEraseResponse;
///
/// let written = include_bytes!(concat!(env!("SMP_VECTORS"), "/image/image_erase_response/1.cbor"));
/// assert_eq!(cbor_static::from_slice::<ImageEraseResponse>(written)?, ImageEraseResponse::default());
/// assert_eq!(cbor_static::from_slice::<ImageEraseResponse>(b"\xa1\x62rc\x00")?.rc, Some(0));
/// # Ok::<(), cbor_static::Error>(())
/// ```
#[derive(Cbor, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cbor(length_first, deny_unknown_fields)]
pub struct ImageEraseResponse {
    #[cbor(skip_serializing_if = "Option::is_none")]
    pub rc: Option<i64>,
}

/// Set state of image's response: the states after the write, shaped as the read's response.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageStatesWriteResponse;
///
/// let built = include_bytes!(concat!(env!("SMP_VECTORS"), "/hand_built/image_states_write_response.cbor"));
/// assert_eq!(cbor_static::from_slice::<ImageStatesWriteResponse>(built)?.images.len(), 2);
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type ImageStatesWriteResponse<'a> = ImageStatesReadResponse<'a>;

/// Slot info (command 6): no fields.
///
/// ```
/// use cbor_static_conformance::smp::image::ImageSlotInfoReadRequest;
///
/// let built = include_bytes!(concat!(env!("SMP_VECTORS"), "/hand_built/image_slot_info_read_request.cbor"));
/// cbor_static::from_slice::<ImageSlotInfoReadRequest>(built)?;
/// # Ok::<(), cbor_static::Error>(())
/// ```
pub type ImageSlotInfoReadRequest = super::Empty;
