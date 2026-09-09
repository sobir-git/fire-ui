use std::{path::Path, sync::Arc};
/// Immutable font bytes. Clones share storage; no parser or renderer is initialized.
#[derive(Clone)]
pub struct FontBytes(Arc<Storage>);
enum Storage {
    Owned(Vec<u8>),
    Static(&'static [u8]),
    #[cfg(feature = "mmap")]
    Mapped(memmap2::Mmap),
}
impl FontBytes {
    pub fn owned(bytes: Vec<u8>) -> Self {
        Self(Arc::new(Storage::Owned(bytes)))
    }
    pub fn from_static(bytes: &'static [u8]) -> Self {
        Self(Arc::new(Storage::Static(bytes)))
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        std::fs::read(path)
            .map(Self::owned)
            .map_err(|e| e.to_string())
    }
    /// Map bytes without copying them.
    ///
    /// # Safety
    /// The file must not be modified or truncated until every clone and all derived
    /// renderer resources have been dropped.
    #[cfg(feature = "mmap")]
    pub unsafe fn map(path: impl AsRef<Path>) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        Ok(Self(Arc::new(Storage::Mapped(unsafe {
            memmap2::Mmap::map(&file).map_err(|e| e.to_string())?
        }))))
    }
}
impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        match self.0.as_ref() {
            Storage::Owned(v) => v,
            Storage::Static(v) => v,
            #[cfg(feature = "mmap")]
            Storage::Mapped(v) => v,
        }
    }
}
impl std::borrow::Borrow<[u8]> for FontBytes {
    fn borrow(&self) -> &[u8] {
        self.as_ref()
    }
}
/// A face in a font file or collection. Consumers validate the formats they support.
#[derive(Clone)]
pub struct Font {
    pub bytes: FontBytes,
    /// Zero-based face within the font file, not the paragraph's resource-list index.
    pub face_index: u32,
}
/// Ordered resources; paragraph font indices address entries in this list.
/// Pass clones of the same list to text layout and drawing so glyph IDs agree.
/// Immutable byte owners can be shared with a custom shaping worker without copying.
#[derive(Clone, Default)]
pub struct Fonts(Arc<Vec<Font>>);
impl Fonts {
    pub fn new(fonts: impl IntoIterator<Item = Font>) -> Self {
        Self(Arc::new(fonts.into_iter().collect()))
    }
    pub fn load(paths: &[impl AsRef<Path>]) -> Result<Self, String> {
        paths
            .iter()
            .map(|p| {
                FontBytes::load(p).map(|bytes| Font {
                    bytes,
                    face_index: 0,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self::new)
    }
    /// Map the first face of each selected file.
    ///
    /// # Safety
    /// Every file must remain unmodified until all byte and renderer owners drop.
    #[cfg(feature = "mmap")]
    pub unsafe fn map(paths: &[impl AsRef<Path>]) -> Result<Self, String> {
        paths
            .iter()
            .map(|p| {
                unsafe { FontBytes::map(p) }.map(|bytes| Font {
                    bytes,
                    face_index: 0,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self::new)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&Font> {
        self.0.get(index)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Font> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_and_static_bytes_keep_the_original_storage_and_face_selection() {
        let owned = vec![1, 2, 3, 4];
        let pointer = owned.as_ptr();
        let bytes = FontBytes::owned(owned);
        assert_eq!(bytes.as_ref().as_ptr(), pointer);
        static STATIC: &[u8] = &[9, 8, 7];
        let fixed = FontBytes::from_static(STATIC);
        assert_eq!(fixed.as_ref().as_ptr(), STATIC.as_ptr());
        let fonts = Fonts::new([
            Font {
                bytes,
                face_index: 3,
            },
            Font {
                bytes: fixed,
                face_index: 1,
            },
        ]);
        let retained = fonts.get(0).unwrap().bytes.clone();
        assert_eq!(fonts.get(0).unwrap().face_index, 3);
        assert_eq!(fonts.get(1).unwrap().face_index, 1);
        drop(fonts);
        assert_eq!(retained.as_ref(), [1, 2, 3, 4]);
    }
    #[test]
    fn immutable_resources_can_be_shared_with_custom_workers() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<FontBytes>();
        send_sync::<Fonts>();
        let fonts = Fonts::new([Font {
            bytes: FontBytes::from_static(&[1, 2]),
            face_index: 0,
        }]);
        let worker = fonts.clone();
        std::thread::spawn(move || assert_eq!(worker.get(0).unwrap().bytes.as_ref(), &[1, 2]))
            .join()
            .unwrap();
        assert_eq!(fonts.len(), 1);
    }
    #[test]
    fn empty_resources_do_not_require_a_font_format_or_mapping() {
        assert!(Fonts::default().is_empty());
        assert!(Fonts::new([]).get(0).is_none());
    }
}
