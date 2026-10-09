//! The little-endian D-Bus wire format, limited to the types a tray item uses.

pub const CALL: u8 = 1;
pub const RETURN: u8 = 2;
pub const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;
pub const NO_REPLY: u8 = 1;

#[derive(Default)]
pub struct Writer {
    b: Vec<u8>,
}

impl Writer {
    pub fn into_bytes(self) -> Vec<u8> {
        self.b
    }
    pub fn pad(&mut self, align: usize) {
        while !self.b.len().is_multiple_of(align) {
            self.b.push(0);
        }
    }
    pub fn u8(&mut self, v: u8) {
        self.b.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.pad(4);
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    pub fn bytes(&mut self, v: &[u8]) {
        self.b.extend_from_slice(v);
    }
    /// Strings and object paths.
    pub fn str(&mut self, v: &str) {
        self.u32(v.len() as u32);
        self.b.extend_from_slice(v.as_bytes());
        self.b.push(0);
    }
    pub fn signature(&mut self, v: &str) {
        self.u8(v.len() as u8);
        self.b.extend_from_slice(v.as_bytes());
        self.b.push(0);
    }
    pub fn variant(&mut self, sig: &str, value: impl FnOnce(&mut Self)) {
        self.signature(sig);
        value(self);
    }
    /// An array whose elements align to `align`; the length excludes that padding.
    pub fn array(&mut self, align: usize, items: impl FnOnce(&mut Self)) {
        self.u32(0);
        let at = self.b.len() - 4;
        self.pad(align);
        let start = self.b.len();
        items(self);
        let len = (self.b.len() - start) as u32;
        self.b[at..at + 4].copy_from_slice(&len.to_le_bytes());
    }
}

/// One header field: code, signature (`s`, `o` or `g`) and value.
pub type Field<'a> = (u8, char, &'a str);

pub fn frame(
    kind: u8,
    flags: u8,
    serial: u32,
    fields: &[Field],
    reply_to: Option<u32>,
    sig: &str,
    body: &[u8],
) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&[b'l', kind, flags, 1]);
    w.u32(body.len() as u32);
    w.u32(serial);
    w.array(8, |w| {
        for &(code, ty, value) in fields {
            w.pad(8);
            w.u8(code);
            w.variant(&ty.to_string(), |w| {
                if ty == 'g' {
                    w.signature(value)
                } else {
                    w.str(value)
                }
            });
        }
        if let Some(r) = reply_to {
            w.pad(8);
            w.u8(5);
            w.variant("u", |w| w.u32(r));
        }
        if !sig.is_empty() {
            w.pad(8);
            w.u8(8);
            w.variant("g", |w| w.signature(sig));
        }
    });
    w.pad(8);
    w.bytes(body);
    w.into_bytes()
}

/// Length of the first complete message in `buf`, if one has arrived.
pub fn complete(buf: &[u8]) -> Option<usize> {
    if buf.len() < 16 {
        return None;
    }
    let le = buf[0] == b'l';
    let word = |at: usize| {
        let b = [buf[at], buf[at + 1], buf[at + 2], buf[at + 3]];
        (if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }) as usize
    };
    let len = (16 + word(12)).next_multiple_of(8) + word(4);
    (buf.len() >= len).then_some(len)
}

pub struct Message<'a> {
    pub kind: u8,
    pub flags: u8,
    pub serial: u32,
    pub path: &'a str,
    pub interface: &'a str,
    pub member: &'a str,
    pub sender: &'a str,
    body: &'a [u8],
}

impl<'a> Message<'a> {
    /// Little-endian messages only; the bus daemon relays in the sender's order
    /// and panels on little-endian machines send little-endian.
    pub fn parse(frame: &'a [u8]) -> Option<Self> {
        if frame.first() != Some(&b'l') {
            return None;
        }
        let mut r = Reader::new(frame);
        r.at = 4;
        let _ = r.u32()?;
        let serial = r.u32()?;
        let end = 16 + r.u32()? as usize;
        let mut m = Message {
            kind: frame[1],
            flags: frame[2],
            serial,
            path: "",
            interface: "",
            member: "",
            sender: "",
            body: &[],
        };
        while r.at < end {
            r.align(8);
            let code = r.u8()?;
            match r.signature()? {
                "s" | "o" => {
                    let v = r.str()?;
                    match code {
                        1 => m.path = v,
                        2 => m.interface = v,
                        3 => m.member = v,
                        7 => m.sender = v,
                        _ => {}
                    }
                }
                "g" => {
                    r.signature()?;
                }
                "u" => {
                    r.u32()?;
                }
                _ => return None,
            }
        }
        m.body = frame.get(end.next_multiple_of(8)..)?;
        Some(m)
    }

    pub fn body(&self) -> Reader<'a> {
        Reader::new(self.body)
    }
}

pub struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, at: 0 }
    }
    fn align(&mut self, a: usize) {
        self.at = self.at.next_multiple_of(a);
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.at)?;
        self.at += 1;
        Some(v)
    }
    pub fn u32(&mut self) -> Option<u32> {
        self.align(4);
        let v = self.b.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_le_bytes(v.try_into().ok()?))
    }
    pub fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    pub fn i32_array(&mut self) -> Option<Vec<i32>> {
        let len = self.u32()? as usize;
        let end = self.at + len;
        let mut v = Vec::with_capacity(len / 4);
        while self.at < end {
            v.push(self.i32()?);
        }
        Some(v)
    }
    pub fn str(&mut self) -> Option<&'a str> {
        let len = self.u32()? as usize;
        let v = self.b.get(self.at..self.at + len)?;
        self.at += len + 1;
        std::str::from_utf8(v).ok()
    }
    pub fn signature(&mut self) -> Option<&'a str> {
        let len = self.u8()? as usize;
        let v = self.b.get(self.at..self.at + len)?;
        self.at += len + 1;
        std::str::from_utf8(v).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_parse_back() {
        let mut body = Writer::default();
        body.i32(-5);
        body.str("horizontal");
        let f = frame(
            CALL,
            0,
            7,
            &[
                (1, 'o', "/StatusNotifierItem"),
                (2, 's', "org.kde.StatusNotifierItem"),
                (3, 's', "Scroll"),
                (7, 's', ":1.9"),
            ],
            None,
            "is",
            &body.into_bytes(),
        );
        assert_eq!(complete(&f), Some(f.len()));
        assert_eq!(complete(&f[..f.len() - 1]), None);
        let m = Message::parse(&f).unwrap();
        assert_eq!(
            (m.kind, m.serial, m.interface, m.member, m.sender),
            (CALL, 7, "org.kde.StatusNotifierItem", "Scroll", ":1.9")
        );
        let mut r = m.body();
        assert_eq!((r.i32(), r.str()), (Some(-5), Some("horizontal")));
    }

    #[test]
    fn arrays_exclude_leading_padding_from_their_length() {
        let mut w = Writer::default();
        w.array(8, |w| {
            w.pad(8);
            w.i32(1);
        });
        assert_eq!(w.into_bytes(), [4, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
    }
}
