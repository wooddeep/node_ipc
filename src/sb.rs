use std::iter;
use std::io::Write;
use std::string::FromUtf8Error;

const DEFAULT_CAPACITY: usize = 1024;
const MAX_UNICODE_WIDTH: usize = 4;

/// This is a growable string builder.
#[derive(Debug)]
pub struct Builder(Vec<u8>);

impl Default for Builder {
    fn default() -> Builder {
        let inner = Vec::with_capacity(DEFAULT_CAPACITY);
        Builder(inner)
    }
}

impl Builder {
    /// Return a new `Builder` with an initial capacity.
    pub fn new(size: usize) -> Builder {
        let inner = Vec::with_capacity(size);
        Builder(inner)
    }

    /// Add a type that can be viewed as a slice of bytes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use string_builder::Builder;
    ///
    /// let mut builder = Builder::default();
    /// builder.append("some string");
    /// ```
    pub fn append<T: ToBytes>(&mut self, buf: T) {
        self.0.write_all(&buf.to_bytes()).unwrap()
    }

    pub fn setByte(&mut self, index: usize, data: u8) {
        self.0[index] = data;
    }

    /// Return the current length in bytes of the underlying buffer.
    ///
    /// # Example
    ///
    /// ```rust
    /// use string_builder::Builder;
    ///
    /// let mut builder = Builder::default();
    /// builder.append("four");
    /// assert_eq!(builder.len(), 4);
    /// ```
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return a `String` of our buffer once we are done appending to it. This method consumes
    /// the underlying buffer.
    ///
    /// # Example
    ///
    /// ```rust
    /// use string_builder::Builder;
    ///
    /// let mut builder = Builder::default();
    /// builder.append("i am building");
    /// builder.append(' ');
    /// builder.append("a string");
    /// assert_eq!(builder.string().unwrap(), "i am building a string");
    /// ```
    pub fn string(self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.0)
    }
}

/// A trait to convert a value into a byte slice that can be appended to a `Builder`.
pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

// Generate a buffer with the same length as the given argument in order to use `copy_from_slice`.
fn make_copyable_buf(len: usize) -> Vec<u8> {
    iter::repeat(0).take(len).collect::<Vec<u8>>()
}

// Copy the slice into a `Vec` with the same capacity.
fn slice_to_vec(s: &[u8]) -> Vec<u8> {
    let mut res = make_copyable_buf(s.len());
    res.copy_from_slice(s);
    res
}

impl ToBytes for String {
    fn to_bytes(&self) -> Vec<u8> {
        slice_to_vec(self.as_bytes())
    }
}

impl<'a> ToBytes for &'a str {
    fn to_bytes(&self) -> Vec<u8> {
        slice_to_vec(self.as_bytes())
    }
}

impl ToBytes for u8 {
    fn to_bytes(&self) -> Vec<u8> {
        vec![*self]
    }
}

impl ToBytes for char {
    fn to_bytes(&self) -> Vec<u8> {
        // The maximum length of a unicode character is 4 bytes.
        let mut buf = [0; MAX_UNICODE_WIDTH];
        slice_to_vec(self.encode_utf8(&mut buf).as_bytes())
    }
}

impl<'a> ToBytes for &'a [u8] {
    fn to_bytes(&self) -> Vec<u8> {
        slice_to_vec(self)
    }
}

#[cfg(test)]
mod tests {
    use super::Builder;

    #[test]
    fn test_all_supported_types() {
        let mut b = Builder::default();
        b.append(String::from("hello"));
        b.append(',');
        b.append(b' ');
        b.append("world");
        b.append(" it works".as_bytes());

        assert_eq!(b.string().unwrap(), "hello, world it works");
    }

    #[test]
    fn test_individual_unicode_characters() {
        let mut b = Builder::default();
        b.append('‘');
        b.append("starts with and ends with");
        b.append('‗');

        assert_eq!(b.string().unwrap(), "‘starts with and ends with‗");
    }

    #[test]
    fn test_tool_album() {
        let mut b = Builder::default();
        b.append('\u{00C6}');
        b.append("nima");

        assert_eq!(b.string().unwrap(), "\u{00C6}nima");
    }
}
