use std::io::{
  Write,
  Read,
  Cursor,
  Result,
  Error,
  ErrorKind,
};
use ::{
  ExtWrite,
  ExtRead,
  CompW,
};
use super::{
  test_extwr,
  test_comp_one,
};

#[test]
fn test_endstream () {
  test_extwr(EndStream::new(2), EndStream::new(2),
  2,
  &[&vec![1,2,3],&vec![4,5],&vec![6,7,8],&vec![9]],
  &[1,2,3,4,5,6,7,8,9]
  ).unwrap();
  test_comp_one(EndStream::new(1), EndStream::new(1),
  3,
  &[&vec![1,2,3],&vec![4,5],&vec![6,7,8],&vec![9]],
  &[1,2,3,4,5,6,7,8,9]
  ).unwrap();

}



/// a writer with end byte usage (we read content and need to stop at some point without knowing
/// what is in content). After N read byte if 0 end if 1 continu for N next bytes.
/// First is window size (renewable with padding if flushed or end).
/// Second is counter of byte written for this window.
pub struct EndStream(usize,usize); 


pub type CEndStream<'a,'b,A> = CompW<'a,'b,EndStream,A>;

impl EndStream {
  pub fn new(winsize : usize) -> Self { EndStream(winsize, winsize) }
}

impl<W : Write> ExtWrite<W> for EndStream {
  #[inline]
  fn write_header(&mut self, _ : &mut W) -> Result<()> {self.1 = self.0; Ok(())}
  #[inline]
  fn write_into(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    let mut ix = 0;
    while ix < cont.len() {
      let l = if self.1 + ix < cont.len() {
        try!(w.write(&cont[ix..ix + self.1]))
      } else {
        try!(w.write(&cont[ix..]))
      };
      ix += l;
      self.1 -= l;
      if self.1 == 0 {
        // non 0 (terminal) value
        try!(w.write(&[1]));
        self.1 = self.0;
      }
    };
    Ok(ix)
  }

  #[inline]
  fn write_end(&mut self, r : &mut W) -> Result<()> {
    // padd
    let mut buffer = [0; 256];
    while self.1 != 0 {
      let l = if self.1 > 256 {
        try!(r.write(&mut buffer))
      } else {
        try!(r.write(&mut buffer[..self.1]))
      };
      self.1 -= l;
    }
    // terminal 0
    try!(r.write(&[0]));
    Ok(())
  }
}

impl<R : Read> ExtRead<R> for EndStream {
  #[inline]
  fn read_header(&mut self, _ : &mut R) -> Result<()> {
    self.1 = self.0; 
    Ok(())}
  #[inline]
  fn read_from(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    if self.1 == 0 {

      return Ok(0)
    }
    let l = if self.1 < buf.len() {
      try!(r.read(&mut buf[..self.1]))
    } else {
      try!(r.read(buf))
    };
 
    self.1 = self.1 - l;
    if self.1 == 0 {
      let mut b = [0];
      let rr = try!(r.read(&mut b));
      if rr != 1 {
        return
         Err(Error::new(ErrorKind::Other, "No bytes after window size, do not know if ended or repeat"));
      }
      if b[0] == 0 {
        // ended window, need header for next (stuck to ret 0 up to read_end call)
        // the point of this write : getting a read at 0 at some point for unknow content read (for
        // instance encyphered bytes).
        return Ok(0)
      } else {
        // read next window
        self.1 = self.0;
      }
    };
    Ok(l)
  }
  #[inline]
  fn read_end(&mut self, r : &mut R) -> Result<()> {
    if self.1 == 0 {
      self.1 = self.0;
      Ok(())
    } else {
      let mut buffer = [0; 256];
      while self.1 != 0 {
        let l = if self.1 > 256 {
          try!(r.read(&mut buffer))
        } else {
          try!(r.read(&mut buffer[..self.1]))
        };
        self.1 -= l;
      }
      let ww = try!(r.read(&mut buffer[..1]));
      if ww != 1 || buffer[0] != 0 {
        println!("ww{}",buffer[0]);
        Err(Error::new(ErrorKind::Other, "End read does not find expected terminal 0 of widows"))
      } else {
        self.1 = self.0;
        Ok(())
      }
    }
  }

}


