# Fire UI fonts

Immutable font bytes and explicit face selection, independent of shaping and rendering.
`FontBytes::owned` takes an existing allocation; `FontBytes::from_static` references embedded bytes.
Clones share immutable storage and can cross threads. `Fonts::load` reads selected files.
The optional `mmap` feature adds unsafe file mapping when the caller guarantees the files
remain unchanged for every byte and renderer owner's lifetime.

`Fonts::new` accepts `Font { bytes, face_index }` entries. `face_index` selects a face
inside its file; paragraph font indices select entries in the ordered `Fonts` list.
Pass clones of that same list to the text engine and renderer. Each consumer validates
its supported formats. Cairo supports file face indices through 65535; larger indices
cannot be represented by FreeType without colliding with its variation-instance bits.

`system_font()` returns one common installed font or the `FIRE_UI_FONT` override.
It only runs when an application calls it; nothing in Fire UI discovers fonts itself.
