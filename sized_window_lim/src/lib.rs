extern crate rand;
extern crate readwrite_comp;
extern crate byteorder;
use rand::OsRng;
use rand::Rng;
use std::io::{
  Write,
  Read,
  Result,
  Error as IoError,
  ErrorKind as IoErrorKind,
};
use readwrite_comp::{
  ExtRead,
  ExtWrite,
};


use std::marker::PhantomData;
use byteorder::{
  LittleEndian,
  ReadBytesExt,
  WriteBytesExt,
};

/// conf trait
pub trait SizedWindowsParams {
  const INIT_SIZE : usize;
  const GROWTH_RATIO : Option<(usize,usize)>;
  /// size of window is written, this way INIT_size and growth_ratio may diverge (multiple
  /// profiles)
  const WRITE_SIZE : bool;
  /// could disable random padding, default to enable for sec consideration
  const SECURE_PAD : bool = true;
}

#[derive(Clone)]
pub struct SizedWindows<P : SizedWindowsParams>  {
  init_size : usize, // TODO rename to last_size
  winrem : usize,
  _p : PhantomData<P>,
}

impl<P : SizedWindowsParams> SizedWindows<P> {
  /// p param is only to simplify type inference (it is an empty struct)
  pub fn new (_ : P) -> Self {
    SizedWindows {
      init_size : P::INIT_SIZE,
      winrem : P::INIT_SIZE,
      _p : PhantomData,
    }
  }
  #[inline]
  fn next_winsize<R : Read> (&mut self, r : &mut R ) -> Result<()>{
    // winrem for next
    self.winrem = if P::WRITE_SIZE {
      try!(r.read_u64::<LittleEndian>()) as usize
    } else {
      match P::GROWTH_RATIO {
         Some((n,d)) => self.init_size * n / d,
         None => P::INIT_SIZE,
      }
    };
    self.init_size = self.winrem;
    Ok(())

  }
}

impl<P : SizedWindowsParams> ExtWrite for SizedWindows<P> {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    if P::WRITE_SIZE {
      try!(w.write_u64::<LittleEndian>(self.winrem as u64));
    }
//    self.init_size = self.winrem;
    Ok(())
  }

  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    let mut tot = 0;
    while tot < cont.len() {
      if self.winrem == 0 {

        // init next winrem
        self.winrem = match P::GROWTH_RATIO {
            Some((n,d)) => self.init_size * n / d,
            None => P::INIT_SIZE,
        };

        self.init_size = self.winrem;
        // non 0 (terminal) value
        try!(w.write(&[1]));
        if P::WRITE_SIZE {
          try!(w.write_u64::<LittleEndian>(self.winrem as u64));
        };
      }



      let ww = if self.winrem + tot < cont.len() {
        try!(w.write(&cont[tot..tot + self.winrem]))
      } else {
        try!(w.write(&cont[tot..]))
      };
      tot += ww;
      self.winrem -= ww;
    }
    Ok(tot)
  }

  #[inline]
  fn write_end<W : Write>(&mut self, r : &mut W) -> Result<()> {
    // TODO buffer is not nice here and more over we need random content (nice for debugging
    // without but in tunnel it gives tunnel length) -> !!!
    let mut buffer = [0; 256];

    if P::SECURE_PAD {
      let mut rng = try!(OsRng::new()); // TODO test for perf (if cache)
      rng.fill_bytes(&mut buffer);
    };
    while self.winrem != 0 {
      let ww = if self.winrem > 256 {
        try!(r.write(&mut buffer))
      } else {
        try!(r.write(&mut buffer[..self.winrem]))
      };
      self.winrem -= ww;
    }
    // terminal 0
    try!(r.write(&[0]));
    // init as new
    self.init_size = P::INIT_SIZE;
    self.winrem = P::INIT_SIZE;
    Ok(())
  }

}


impl<P : SizedWindowsParams> ExtRead for SizedWindows<P> {
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    if P::WRITE_SIZE {
      self.winrem = try!(r.read_u64::<LittleEndian>()) as usize;
      self.init_size = self.winrem;
    }
    Ok(())
  }

  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    if self.init_size == 0 {
      // ended read (still padded)
      return Ok(0);
    }
    let rr = if self.winrem < buf.len() {
      try!(r.read(&mut buf[..self.winrem]))
    } else {
      try!(r.read(buf))
    };
    self.winrem -= rr;
    if self.winrem == 0 {
      let mut b = [0];
      let rb = try!(r.read(&mut b));
      if rb != 1 {
        return
         Err(IoError::new(IoErrorKind::Other, "No bytes after window size, do not know if ended or repeat"));
      }
      if b[0] == 0 {
        // ended (case where there is no padding or we do not know what we read and read also
        // the padding)
        self.init_size = 0;
        return Ok(rr)
      } else {
        // new window and drop this byte
        try!(self.next_winsize(r));
      }
    }
    Ok(rr)
  }
  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {

    if self.winrem == 0 {
      self.init_size = P::INIT_SIZE;
      self.winrem = P::INIT_SIZE;
      Ok(())
    } else {
      println!("winrem:{:?}",self.winrem);
    // TODO buffer is needed here -> see if Read interface should not have a fn drop where we read
      // without buffer and drop content. For now hardcoded buffer length...
      let mut buffer = [0; 256];
      buffer[0] = 1;
      while buffer[0] != 0 {

        while self.winrem != 0 {
          let ww = if self.winrem > 256 {
            try!(r.read(&mut buffer))
          } else {
            try!(r.read(&mut buffer[..self.winrem]))
          };
          if ww ==  0 {
//           error!("read end pading : missing {}",self.winrem);
           return
             Err(IoError::new(IoErrorKind::Other, "End read missing padding"));
          }
          self.winrem -= ww;
        }

        let ww = try!(r.read(&mut buffer[..1]));
        if ww != 1  {
 //          error!("read end no terminal 0 : nbread {}\n{}\n",ww,buffer[0]);
           return
             Err(IoError::new(IoErrorKind::Other, "End read does not find expected terminal 0 of windows"));
        }
        if buffer[0] != 0 {
          try!(self.next_winsize(r));
        }
      }
      // init as new
      self.init_size = P::INIT_SIZE;
      self.winrem = P::INIT_SIZE;

      Ok(())
    }
  }



}

#[cfg(test)]
mod test {

  extern crate readwrite_comp_test;
  use self::readwrite_comp_test::test_bytes_wr;
  use super::{
    SizedWindowsParams,
    SizedWindows,
  };
  struct Params1;
  struct Params2;
  struct Params3;
  struct Params4;

  impl SizedWindowsParams for Params1 {
      const INIT_SIZE : usize = 20;
      const GROWTH_RATIO : Option<(usize,usize)> = Some((4,3));
      const WRITE_SIZE : bool = false;
  }
  impl SizedWindowsParams for Params2 {
      const INIT_SIZE : usize = 20;
      const GROWTH_RATIO : Option<(usize,usize)> = None;
      const WRITE_SIZE : bool = false;
  }
  impl SizedWindowsParams for Params3 {
      const INIT_SIZE : usize = 20;
      const GROWTH_RATIO : Option<(usize,usize)> = Some((4,3));
      const WRITE_SIZE : bool = true;
  }
  impl SizedWindowsParams for Params4 {
      const INIT_SIZE : usize = 20;
      const GROWTH_RATIO : Option<(usize,usize)> = None;
      const WRITE_SIZE : bool = true;
  }



  #[test]
  fn windows_test () {
    let mut et = SizedWindows::new(Params2);
    let mut et2 = SizedWindows::new(Params2);
    test_bytes_wr(
      150,
      15,
      &mut et,
      &mut et2,
    ).unwrap();
    test_bytes_wr(
      150,
      36,
      &mut et,
      &mut et2,
    ).unwrap();

    let mut et = SizedWindows::new(Params1);
    let mut et2 = SizedWindows::new(Params1);
    test_bytes_wr(
      150,
      7,
      &mut et,
      &mut et2,
    ).unwrap();

    let mut et = SizedWindows::new(Params2);
    let mut et2 = SizedWindows::new(Params2);
    test_bytes_wr(
      150,
      15,
      &mut et,
      &mut et2,
    ).unwrap();
    let mut et = SizedWindows::new(Params3);
    let mut et2 = SizedWindows::new(Params3);
    test_bytes_wr(
      150,
      200,
      &mut et,
      &mut et2,
    ).unwrap();
    let mut et = SizedWindows::new(Params4);
    let mut et2 = SizedWindows::new(Params4);
    test_bytes_wr(
      150,
      7,
      &mut et,
      &mut et2,
    ).unwrap();
  }


}

