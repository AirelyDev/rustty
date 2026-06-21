/// A single entry in the magic-byte / extension detection table.
///
/// `sigs` is a list of signatures; each signature is a list of optional bytes
/// where `None` is a wildcard (matches any byte) — this mirrors the Zig
/// `[]const ?u8` representation.
struct TypeDetail {
    typ: FileType,
    sigs: &'static [&'static [Option<u8>]],
    exts: &'static [&'static str],
}

const TYPE_DETAILS: &[TypeDetail] = &[
    TypeDetail {
        typ: FileType::Jpeg,
        sigs: &[
            &[Some(0xFF), Some(0xD8), Some(0xFF), Some(0xDB)],
            &[
                Some(0xFF), Some(0xD8), Some(0xFF), Some(0xE0), Some(0x00), Some(0x10),
                Some(0x4A), Some(0x46), Some(0x49), Some(0x46), Some(0x00), Some(0x01),
            ],
            &[Some(0xFF), Some(0xD8), Some(0xFF), Some(0xEE)],
            &[
                Some(0xFF), Some(0xD8), Some(0xFF), Some(0xE1), None, None,
                Some(0x45), Some(0x78), Some(0x69), Some(0x66), Some(0x00), Some(0x00),
            ],
            &[Some(0xFF), Some(0xD8), Some(0xFF), Some(0xE0)],
        ],
        exts: &[".jpg", ".jpeg", ".jfif"],
    },
    TypeDetail {
        typ: FileType::Png,
        sigs: &[&[
            Some(0x89), Some(0x50), Some(0x4E), Some(0x47), Some(0x0D), Some(0x0A),
            Some(0x1A), Some(0x0A),
        ]],
        exts: &[".png"],
    },
    TypeDetail {
        typ: FileType::Gif,
        sigs: &[
            &[Some(b'G'), Some(b'I'), Some(b'F'), Some(b'8'), Some(b'7'), Some(b'a')],
            &[Some(b'G'), Some(b'I'), Some(b'F'), Some(b'8'), Some(b'9'), Some(b'a')],
        ],
        exts: &[".gif"],
    },
    TypeDetail {
        typ: FileType::Bmp,
        sigs: &[&[Some(b'B'), Some(b'M')]],
        exts: &[".bmp"],
    },
    TypeDetail {
        typ: FileType::Qoi,
        sigs: &[&[Some(b'q'), Some(b'o'), Some(b'i'), Some(b'f')]],
        exts: &[".qoi"],
    },
    TypeDetail {
        typ: FileType::Webp,
        sigs: &[&[
            Some(0x52), Some(0x49), Some(0x46), Some(0x46), None, None, None, None,
            Some(0x57), Some(0x45), Some(0x42), Some(0x50),
        ]],
        exts: &[".webp"],
    },
];

/// This is a helper for detecting file types based on magic bytes.
///
/// Ref: https://en.wikipedia.org/wiki/List_of_file_signatures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// JPEG image file.
    Jpeg,

    /// PNG image file.
    Png,

    /// GIF image file.
    Gif,

    /// BMP image file.
    Bmp,

    /// QOI image file.
    Qoi,

    /// WebP image file.
    Webp,

    /// Unknown file format.
    Unknown,
}

impl FileType {
    /// Detect file type based on the magic bytes
    /// at the start of the provided file contents.
    pub fn detect(contents: &[u8]) -> FileType {
        for typ in TYPE_DETAILS {
            'sig: for signature in typ.sigs {
                if contents.len() >= signature.len() {
                    for (f, sig) in contents[0..signature.len()].iter().zip(signature.iter()) {
                        if let Some(s) = sig {
                            if *f != *s {
                                continue 'sig;
                            }
                        }
                    }
                    return typ.typ;
                }
            }
        }
        FileType::Unknown
    }

    /// Guess file type from its extension.
    #[allow(non_snake_case)]
    pub fn guessFromExtension(extension: &[u8]) -> FileType {
        for typ in TYPE_DETAILS {
            for ext in typ.exts {
                if extension.eq_ignore_ascii_case(ext.as_bytes()) {
                    return typ.typ;
                }
            }
        }
        FileType::Unknown
    }
}
