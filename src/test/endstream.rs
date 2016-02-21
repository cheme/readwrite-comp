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
  //MultiW,
  MultiWExt,
  new_multiw,
  //MultiR,
  MultiRExt,
  new_multir,
};
use super::{
  test_extwr,
  test_comp_one,
};
#[test]
fn test_read_end_multi_pad () {
  let mut ew = EndStream::new(2);
  let mut er = EndStream::new(2);
  let mut content = vec![1,2,3];
  let mut oute = Cursor::new(Vec::new());
{ 
  let out = &mut oute;
  ew.write_header(out).unwrap();
  ew.write_into(out, &mut content).unwrap();
  ew.write_end(out).unwrap();
  ew.flush_into(out).unwrap();
  assert!(out.get_ref().len() == 6); // 1 2 1 (for next) 3 rpad 0
}
  oute.set_position(0);
  assert!(er.read_end(&mut oute).is_ok());
 
}
 
#[test]
fn test_constant_size () {
  let mut ew = EndStream::new(2);
  let mut ew2 = EndStream::new(2);
  let mut er = EndStream::new(2);
  let mut content = vec![1,2,3];
  let mut oute = Cursor::new(Vec::new());
{ 
  let out = &mut oute;
  ew.write_header(out).unwrap();
  ew.write_into(out, &mut content).unwrap();
  ew.write_end(out).unwrap();
  ew.flush_into(out).unwrap();
  assert!(out.get_ref().len() == 6); // 1 2 1 (for next) 3 rpad 0
}
  oute.set_position(0);
  let mut ine =  oute;
  let mut oute = Cursor::new(Vec::new());

{ 
  let inp = &mut ine;
  let out = &mut oute;
  let mut buf = vec![0;3];
  let mut sr = 1;
  er.read_header(inp).unwrap();
  ew2.write_header(out).unwrap();
  while sr != 0 {
    sr = er.read_from(inp,&mut buf[..]).unwrap();
    if sr != 0 {
    ew2.write_into(out,&buf[..sr]).unwrap();
    }
  }
  er.read_end(inp).unwrap();
  ew2.write_end(out).unwrap();
  ew2.flush_into(out).unwrap();
  assert!(out.get_ref().len() == 6); // 1 2 1 (for next) 3 rpad 0
}
}


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


#[test]
fn test_multendstream_dec_windows () {
  let c1 = EndStream::new(2);
  let c2 = EndStream::new(3);
  let c3 = EndStream::new(4);
  let ciphs = vec![c3,c2,c1];
  let mut w = Cursor::new(Vec::new());
  { // write end in drop
    let mut mciphsext = MultiWExt::new(ciphs);
    let mut mciphs = new_multiw(&mut w, &mut mciphsext);
    mciphs.write(&[123]).unwrap();
    mciphs.write_end().unwrap();
    mciphs.write(&[25]).unwrap();
  };
  //println!("debug mciphs {:?}",w.get_ref());
//[123, 0, 1, 0, 0, 1, 0, 0, 0, 25, 0, 1, 0, 0, 1, 0, 0, 0]

//  assert!(&[123,0,1,

  let c1 = EndStream::new(2);
  let c2 = EndStream::new(3);
  let c3 = EndStream::new(4);
  let ciphs = vec![c3,c2,c1];
 
  // bigger buf than content
  let mut buf = vec![0;w.get_ref().len() + 10];
 
  let mut w = Cursor::new(w.into_inner());
  { 
    let mut mciphsext = MultiRExt::new(ciphs);
    let mut mciphs = new_multir(&mut w, &mut mciphsext);
    let mut  r = mciphs.read(&mut buf[..]).unwrap();
    assert!(buf[0] == 123);
    // consume all kind of padding
    while r != 0 {
      r = mciphs.read(&mut buf[..]).unwrap();
    }
    assert!(mciphs.read(&mut buf[..]).unwrap() == 0);
    // manual read end to catch error
    assert!(mciphs.read_end().is_ok());
    assert!(buf[0] != 25);
    r = mciphs.read(&mut buf[..]).unwrap();
    // has it sop before
    assert!(buf[0] == 25);
    while r != 0 {
      r = mciphs.read(&mut buf[..]).unwrap();
    }
    assert!(mciphs.read(&mut buf[..]).unwrap() == 0);
  };

}

#[test]
fn test_multendstream_inc_windows () {
  let c1 = EndStream::new(7);
  let c2 = EndStream::new(4);
  let c3 = EndStream::new(2);
  let ciphs = vec![c3,c2,c1];
  let mut w = Cursor::new(Vec::new());
  { // write end in drop
    let mut mciphsext = MultiWExt::new(ciphs);
    let mut mciphs = new_multiw(&mut w, &mut mciphsext);
    mciphs.write(&[123]).unwrap();
    mciphs.write_end().unwrap();
    mciphs.write(&[25]).unwrap();
  };
  let c1 = EndStream::new(7);
  let c2 = EndStream::new(4);
  let c3 = EndStream::new(2);
  let ciphs = vec![c3,c2,c1];
 
  // bigger buf than content
  let mut buf = vec![0;w.get_ref().len() + 10];
 
  let mut w = Cursor::new(w.into_inner());
  { 
    let mut mciphsext = MultiRExt::new(ciphs);
    let mut mciphs = new_multir(&mut w, &mut mciphsext);
    let mut  r = mciphs.read(&mut buf[..]).unwrap();
    assert!(buf[0] == 123);
    // consume all kind of padding
    while r != 0 {
      r = mciphs.read(&mut buf[..]).unwrap();
    }
    assert!(mciphs.read(&mut buf[..]).unwrap() == 0);
    // manual read end to catch error
    assert!(mciphs.read_end().is_ok());
    assert!(buf[0] != 25);
    r = mciphs.read(&mut buf[..]).unwrap();
    // has it sop before
    assert!(buf[0] == 25);
    while r != 0 {
      r = mciphs.read(&mut buf[..]).unwrap();
    }
    assert!(mciphs.read(&mut buf[..]).unwrap() == 0);
  };

}

/// a writer with end byte usage (we read content and need to stop at some point without knowing
/// what is in content). After N read byte if 0 end if 1 continu for N next bytes.
/// First is window size (renewable with padding if flushed or end).
/// Second is counter of byte written for this window.
#[derive(Clone)]
pub struct EndStream(usize,usize); 

pub type CEndStream<'a,'b,A> = CompW<'a,'b,A,EndStream>;

impl EndStream {
  pub fn new(winsize : usize) -> Self { EndStream(winsize, winsize) }
}

impl ExtWrite for EndStream {
  #[inline]
  fn write_header<W : Write>(&mut self, _ : &mut W) -> Result<()> {self.1 = self.0; Ok(())}
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    let mut ix = 0;
    while ix < cont.len() {

      if self.1 == 0 {
        // non 0 (terminal) value
        try!(w.write(&[1]));
        self.1 = self.0;
      }

      let l = if self.1 + ix < cont.len() {
        try!(w.write(&cont[ix..ix + self.1]))
      } else {
        try!(w.write(&cont[ix..]))
      };
      ix += l;
      self.1 -= l;
    };
    Ok(ix)
  }

  #[inline]
  fn write_end<W : Write>(&mut self, r : &mut W) -> Result<()> {
    // padd with 2 for easier frame read (0 stop 1 continue
    let mut buffer = [2; 256];
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

impl ExtRead for EndStream {
  #[inline]
  fn read_header<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    self.1 = self.0; 
    Ok(())}
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {

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
        return Ok(l)
      } else {
        // read next window
        self.1 = self.0;
      }
    };
    Ok(l)
  }
  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    if self.1 == 0 {
      self.1 = self.0;
      Ok(())
    } else {
      let mut buffer = [0; 256];
      buffer[0] = 1;
      while buffer[0] != 0 {

        while self.1 != 0 {
          let l = if self.1 > 256 {
            try!(r.read(&mut buffer))
          } else {
            try!(r.read(&mut buffer[..self.1]))
          };
          self.1 -= l;
        }

        let ww = try!(r.read(&mut buffer[..1]));
        self.1 = self.0;
        if ww != 1  {
          return Err(Error::new(ErrorKind::Other, "End read does not find expected terminal 0 of windows"))
        }
      }
      Ok(())
    }
  }

}


