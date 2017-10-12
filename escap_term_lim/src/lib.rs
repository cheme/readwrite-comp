
//! an end byte is added at the end
//! This sequence is escaped in stream through escape bytes
//! Mostly for test purpose (read byte per byte). Less usefull now that bytes_wr does not have its
//! own traits anymore but is simply ExtWrite and ExtRead for Composable use

extern crate readwrite_comp;
extern crate readwrite_comp_test;
  // TODO if esc char check for esc seq
  // if esc char in content wr esc two times
use std::io::{
  Write,
  Read,
  Result,
};
use readwrite_comp::{
  ExtRead,
  ExtWrite,
};
#[cfg(test)]
use readwrite_comp_test::{
  test_bytes_wr,
};

/// contain esc byte, and if escaped, and if waiting for end read
pub struct EscapeTerm (u8,bool,bool);

impl EscapeTerm {
  pub fn new (t : u8) -> Self {
    EscapeTerm (t,false,false)
  }
}

impl ExtRead for EscapeTerm {
  #[inline]
  fn read_header<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    Ok(())
  }
  /// return 0 if ended (content might still be read afterward on reader but endof BytesWR
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    let mut b = [0];
    if self.2 {
      return Ok(0);
    }
    let mut i = 0;
    while i < buf.len() {
      let rr = try!(r.read(&mut b[..]));
      if rr == 0 {
        return Ok(i);
      }
      if b[0] == self.0 {
        if self.1 {
        //debug!("An escaped char read effectively");
          buf[i] = b[0];
          i += 1;
          self.1 = false;
        } else {
        //debug!("An escaped char read start");
          self.1 = true;
        }
      } else {
        if self.1 {
        //debug!("An escaped end");
          // end
          self.2 = true;
          return Ok(i);
        } else {
          buf[i] = b[0]; 
          i += 1;
        }
      }

    }
    Ok(i)
  }

  /// end read : we know the read is complete (for instance rust serialize object decoded), some
  /// finalize operation may be added (for instance read/drop padding bytes).
  #[inline]
  fn read_end<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    self.2 = false;
    Ok(())
  }

}

impl ExtWrite for EscapeTerm {
  #[inline]
  fn write_header<W : Write>(&mut self, _ : &mut W) -> Result<()> {
    Ok(())
  }

  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    for i in 0.. cont.len() {
      let b = cont[i];
      if b == self.0 {
        //debug!("An escaped char");
        try!(w.write(&cont[i..i+1]));
        try!(w.write(&cont[i..i+1]));
      } else {
        try!(w.write(&cont[i..i+1]));
      }
    }
    Ok(cont.len())
  }

  /// end of content write
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let two = if self.0 == 0 {
      try!(w.write(&[self.0,1]))
    }else {
      try!(w.write(&[self.0,0]))
    };
    // TODO clean io error if two is not 2
    assert!(two == 2);
    Ok(())
  }
}


#[test]
fn escape_test () {
  let mut et = EscapeTerm::new(0);
  let mut et2 = EscapeTerm::new(0);
  test_bytes_wr(
    150,
    7,
    &mut et,
    &mut et2,
  ).unwrap();
  let mut et = EscapeTerm::new(1);
  let mut et2 = EscapeTerm::new(1);
  test_bytes_wr(
    150,
    15,
    &mut et,
    &mut et2,
  ).unwrap();
  let mut et = EscapeTerm::new(3);
  let mut et2 = EscapeTerm::new(3);
  test_bytes_wr(
    150,
    200,
    &mut et,
    &mut et2,
  ).unwrap();
}

