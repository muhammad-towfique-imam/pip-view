//! Media Foundation: camera devices, their streams and modes, and a reader
//! that gives RGB32 frames of 1 stream. Never call these functions on the UI
//! thread, except `startup`: a camera driver can block for a long time.

use anyhow::{anyhow, Result};
use windows::core::{Interface, GUID, HSTRING, PWSTR};
use windows::Win32::Media::KernelStreaming::{PINNAME_VIDEO_CAPTURE, PINNAME_VIDEO_PREVIEW, PINNAME_VIDEO_STILL};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};

use crate::log::log;
use crate::modes::{is_color_subtype, StreamInfo, StreamKind, VideoMode};

/// A camera: the friendly name and the symbolic link (a stable id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub id: String,
}

/// 1 frame: top-down BGRA (the 4th byte is not used), stride = width × 4.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub enum ReadResult {
    Frame(Frame),
    /// No sample this time (a stream tick or a gap).
    Tick,
    /// The camera stopped: an error or the end of the stream.
    Lost,
}

/// The error text with the HRESULT, for the log.
fn hr(what: &str, e: windows::core::Error) -> anyhow::Error {
    anyhow!("{what}: 0x{:08X} {}", e.code().0 as u32, e.message())
}

trait Ctx<T> {
    fn ctx(self, what: &str) -> Result<T>;
}

impl<T> Ctx<T> for windows::core::Result<T> {
    fn ctx(self, what: &str) -> Result<T> {
        self.map_err(|e| hr(what, e))
    }
}

/// Starts Media Foundation 1 time for the process.
pub fn startup() -> Result<()> {
    // SAFETY: MFStartup has no preconditions.
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }.ctx("MFStartup")
}

/// Joins this thread to the multithreaded COM apartment. A second call on the
/// same thread does nothing.
pub fn com_init() {
    // SAFETY: CoInitializeEx has no preconditions; S_FALSE (already joined) is fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn new_attributes(count: u32) -> Result<IMFAttributes> {
    let mut attrs = None;
    // SAFETY: the out pointer is valid.
    unsafe { MFCreateAttributes(&mut attrs, count) }.ctx("MFCreateAttributes")?;
    attrs.ok_or_else(|| anyhow!("MFCreateAttributes gave no object"))
}

fn video_capture_attributes() -> Result<IMFAttributes> {
    let attrs = new_attributes(2)?;
    // SAFETY: valid attribute store and GUID pointers.
    unsafe { attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID) }
        .ctx("set the source type")?;
    Ok(attrs)
}

fn allocated_string(activate: &IMFActivate, key: &GUID) -> Result<String> {
    let mut value = PWSTR::null();
    let mut len = 0u32;
    // SAFETY: the out pointers are valid; the string is freed with CoTaskMemFree.
    unsafe {
        activate.GetAllocatedString(key, &mut value, &mut len).ctx("GetAllocatedString")?;
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0 as *const _));
        Ok(text)
    }
}

/// All video capture devices, in system order.
pub fn list_devices() -> Result<Vec<Device>> {
    com_init();
    let attrs = video_capture_attributes()?;
    let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: the out pointers are valid. Each array item is taken (released)
    // and the array is freed with CoTaskMemFree.
    let devices = unsafe {
        MFEnumDeviceSources(&attrs, &mut array, &mut count).ctx("MFEnumDeviceSources")?;
        let mut devices = Vec::new();
        if !array.is_null() {
            for item in std::slice::from_raw_parts_mut(array, count as usize) {
                if let Some(activate) = item.take() {
                    let name = allocated_string(&activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)?;
                    let id =
                        allocated_string(&activate, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK)?;
                    devices.push(Device { name, id });
                }
            }
            CoTaskMemFree(Some(array as *const _));
        }
        devices
    };
    for d in &devices {
        log!("device: \"{}\" id {}", d.name, d.id);
    }
    Ok(devices)
}

/// Opens the media source of the camera with the symbolic link `id`.
pub fn open_source(id: &str) -> Result<IMFMediaSource> {
    let attrs = video_capture_attributes()?;
    // SAFETY: valid attribute store; the string is copied by SetString.
    unsafe {
        attrs
            .SetString(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, &HSTRING::from(id))
            .ctx("set the symbolic link")?;
        MFCreateDeviceSource(&attrs).ctx("MFCreateDeviceSource")
    }
}

/// Splits a packed UINT64 attribute (frame size, frame rate) into (high, low).
fn split(v: u64) -> (u32, u32) {
    ((v >> 32) as u32, v as u32)
}

/// The mode of a media type, if it has a frame size.
fn mode_of(t: &IMFMediaType) -> Option<VideoMode> {
    // SAFETY: valid media type and GUID pointers.
    unsafe {
        let subtype = t.GetGUID(&MF_MT_SUBTYPE).ok()?;
        let (width, height) = split(t.GetUINT64(&MF_MT_FRAME_SIZE).ok()?);
        let (fps_num, fps_den) = t.GetUINT64(&MF_MT_FRAME_RATE).map(split).unwrap_or((0, 0));
        Some(VideoMode { subtype: subtype.data1, width, height, fps_num, fps_den })
    }
}

/// The video streams of the source. Still streams are left out. Logs each
/// stream with all its modes.
pub fn list_streams(source: &IMFMediaSource) -> Result<Vec<StreamInfo>> {
    let mut streams = Vec::new();
    // SAFETY: COM calls on valid objects with valid out pointers.
    unsafe {
        let pd = source.CreatePresentationDescriptor().ctx("CreatePresentationDescriptor")?;
        let count = pd.GetStreamDescriptorCount().ctx("GetStreamDescriptorCount")?;
        for index in 0..count {
            let mut selected = windows::core::BOOL(0);
            let mut sd = None;
            pd.GetStreamDescriptorByIndex(index, &mut selected, &mut sd).ctx("GetStreamDescriptorByIndex")?;
            let Some(sd) = sd else { continue };
            let handler = sd.GetMediaTypeHandler().ctx("GetMediaTypeHandler")?;
            let major = handler.GetMajorType().ctx("GetMajorType")?;
            if major != MFMediaType_Video {
                log!("stream {index}: not video, left out");
                continue;
            }
            let category = sd.GetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY).ok();
            let kind = match category {
                Some(g) if g == PINNAME_VIDEO_STILL => {
                    log!("stream {index}: still image stream, left out");
                    continue;
                }
                Some(g) if g == PINNAME_VIDEO_PREVIEW => StreamKind::Preview,
                Some(g) if g == PINNAME_VIDEO_CAPTURE => StreamKind::Capture,
                _ => StreamKind::Other,
            };
            let source_types = sd.GetUINT32(&MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES).ok();
            let ir_or_depth = source_types.is_some_and(|t| {
                let t = t as i32;
                t & MFFrameSourceTypes_Color.0 == 0
                    && t & (MFFrameSourceTypes_Infrared.0 | MFFrameSourceTypes_Depth.0) != 0
            });
            let mut modes = Vec::new();
            let type_count = handler.GetMediaTypeCount().ctx("GetMediaTypeCount")?;
            for t in 0..type_count {
                if let Some(mode) = handler.GetMediaTypeByIndex(t).ok().as_ref().and_then(mode_of) {
                    modes.push(mode);
                }
            }
            let color = !ir_or_depth && modes.iter().any(|m| is_color_subtype(m.subtype));
            let list: Vec<String> = modes.iter().map(|m| m.label()).collect();
            log!(
                "stream {index}: {kind:?}, category {category:?}, source types {source_types:?}, color {color}, selected {}, modes: {}",
                selected.as_bool(),
                list.join("; ")
            );
            streams.push(StreamInfo { index, kind, color, modes });
        }
    }
    Ok(streams)
}

/// Opens the camera, lists its streams, and closes it again.
pub fn probe_streams(id: &str) -> Result<Vec<StreamInfo>> {
    com_init();
    let source = open_source(id)?;
    let streams = list_streams(&source);
    // SAFETY: valid source; Shutdown releases the camera.
    unsafe {
        let _ = source.Shutdown();
    }
    streams
}

/// A Source Reader on 1 stream, with RGB32 output.
pub struct Reader {
    reader: IMFSourceReader,
    stream: u32,
    pub width: u32,
    pub height: u32,
    /// Bytes from 1 row to the next in a contiguous buffer. Negative: bottom-up.
    stride: i32,
}

impl Reader {
    /// Selects only `stream`, sets the native type of `mode`, and asks Media
    /// Foundation to convert the frames to RGB32.
    pub fn open(source: &IMFMediaSource, stream: u32, mode: &VideoMode) -> Result<Reader> {
        let attrs = new_attributes(1)?;
        // SAFETY: COM calls on valid objects with valid pointers.
        unsafe {
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).ctx("enable video processing")?;
            let reader =
                MFCreateSourceReaderFromMediaSource(source, &attrs).ctx("MFCreateSourceReaderFromMediaSource")?;
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .ctx("deselect all streams")?;
            reader.SetStreamSelection(stream, true).ctx("select the stream")?;

            let mut native = None;
            for k in 0.. {
                match reader.GetNativeMediaType(stream, k) {
                    Ok(t) if mode_of(&t).as_ref() == Some(mode) => {
                        native = Some(t);
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            let native = native.ok_or_else(|| anyhow!("mode {} not found in stream {stream}", mode.label()))?;
            reader.SetCurrentMediaType(stream, None, &native).ctx("set the native type")?;

            let out = MFCreateMediaType().ctx("MFCreateMediaType")?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ctx("set the major type")?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ctx("set the RGB32 subtype")?;
            reader.SetCurrentMediaType(stream, None, &out).ctx("set the RGB32 output type")?;

            let current = reader.GetCurrentMediaType(stream).ctx("GetCurrentMediaType")?;
            let (width, height) = split(current.GetUINT64(&MF_MT_FRAME_SIZE).ctx("read the frame size")?);
            // Without the attribute, RGB in Media Foundation is bottom-up.
            let stride = current.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or(-(width as i32 * 4));
            log!("reader: stream {stream}, mode {}, output RGB32 {width}x{height}, stride {stride}", mode.label());
            Ok(Reader { reader, stream, width, height, stride })
        }
    }

    /// Reads the next sample. This blocks until a sample comes, the camera
    /// fails, or the media source is shut down.
    pub fn read(&mut self) -> Result<ReadResult> {
        let mut flags = 0u32;
        let mut sample = None;
        // SAFETY: valid reader and out pointers.
        unsafe {
            self.reader
                .ReadSample(self.stream, 0, None, Some(&mut flags), None, Some(&mut sample))
                .ctx("ReadSample")?;
        }
        let stopped = (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_ENDOFSTREAM.0) as u32;
        if flags & stopped != 0 {
            log!("reader: stream flags 0x{flags:X}");
            return Ok(ReadResult::Lost);
        }
        let Some(sample) = sample else { return Ok(ReadResult::Tick) };
        // SAFETY: valid sample; the buffer is unlocked before it is released.
        let buffer = unsafe { sample.ConvertToContiguousBuffer() }.ctx("ConvertToContiguousBuffer")?;
        let row = self.width as usize * 4;
        let mut bgra = vec![0u8; row * self.height as usize];
        if let Ok(b2) = buffer.cast::<IMF2DBuffer>() {
            let mut scan0 = std::ptr::null_mut();
            let mut pitch = 0i32;
            // SAFETY: Lock2D gives row 0 and the pitch (negative for bottom-up);
            // each row has at least `row` bytes.
            unsafe {
                b2.Lock2D(&mut scan0, &mut pitch).ctx("Lock2D")?;
                for y in 0..self.height as usize {
                    let src = scan0.offset(y as isize * pitch as isize);
                    std::ptr::copy_nonoverlapping(src, bgra[y * row..].as_mut_ptr(), row);
                }
                let _ = b2.Unlock2D();
            }
        } else {
            let mut data = std::ptr::null_mut();
            let mut len = 0u32;
            // SAFETY: Lock gives the data and its length; the rows are read
            // only inside the length.
            unsafe {
                buffer.Lock(&mut data, None, Some(&mut len)).ctx("Lock")?;
                let abs = self.stride.unsigned_abs() as usize;
                let h = self.height as usize;
                if abs >= row && abs * h <= len as usize {
                    for y in 0..h {
                        let src_row = if self.stride < 0 { h - 1 - y } else { y };
                        std::ptr::copy_nonoverlapping(data.add(src_row * abs), bgra[y * row..].as_mut_ptr(), row);
                    }
                }
                let _ = buffer.Unlock();
                if abs < row || abs * h > len as usize {
                    return Err(anyhow!("buffer of {len} bytes is too small for {}x{h}", self.width));
                }
            }
        }
        Ok(ReadResult::Frame(Frame { width: self.width, height: self.height, bgra }))
    }
}
