//! Integration tests

use super::{
  ExtWrite,
  ExtRead,
  //CompWState,
  CompRState,
  CompExtR,
  CompExtW,
  CompW,
  CompR,
  //MultiW,
  //MultiR,
  MultiWExt,
  new_multiw,
  MultiRExt,
  new_multir,
};

use std::io::{
  Write,
  Read,
  Cursor,
  Result,
  //Error,
  //ErrorKind,
};

use rand::thread_rng;
use rand::Rng;


pub mod endstream;
pub mod ciph;

use self::ciph::{
  Ciph,
  CCiph,
};

use self::endstream::{
  EndStream,
  CEndStream,
};


/// a composition writer doing nothing
pub struct Void;

impl ExtWrite for Void {
  #[inline]
  fn write_header<W : Write>(&mut self, _ : &mut W) -> Result<()> {Ok(())}
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {w.write(cont)}
  #[inline]
  fn write_end<W : Write>(&mut self, _ : &mut W) -> Result<()> {Ok(())}
}
impl ExtRead for Void {
  #[inline]
  fn read_header<R : Read>(&mut self, _ : &mut R) -> Result<()> {Ok(())}
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    r.read(buf)
  }
  #[inline]
  fn read_end<R : Read>(&mut self, _ : &mut R) -> Result<()> {Ok(())}

}


fn test_extwr<T : ExtWrite + ExtRead>(
mut w : T,
mut r : T,
buf_len : usize,
input : &[&[u8];4],
expect : &[u8],
) -> Result<()> {
  let size_first = input[0].len() + input[1].len();
  let size_second = input[2].len() + input[3].len();
  let mut cout = Cursor::new(Vec::new());
  {
  let c = &mut cout;
  try!(w.write_header(c));
  assert!(input[0].len() == try!(w.write_into(c, input[0])));
  assert!(input[1].len() == try!(w.write_into(c, input[1])));
  try!(w.write_end(c));
  try!(w.write_header(c));
  assert!(input[2].len() == try!(w.write_into(c, input[2])));
  assert!(input[3].len() == try!(w.write_into(c, input[3])));
  try!(w.write_end(c));
  }

  let mut cin = cout;
  cin.set_position(0);
  let c = &mut cin;
  let mut res = Vec::new();
  try!(r.read_header(c));
  let mut rl = size_first;
  let mut l;
  let mut vbuf = vec![0;buf_len];
  let mut first = true;
  let buf = &mut vbuf[..];
  while {
    l = if rl < buf.len() {
      try!(r.read_from(c, &mut buf[..rl]))
    } else {
      try!(r.read_from(c, buf))
    };
    l != 0
  } {
    res.extend_from_slice(&buf[..l]);
    rl = rl - l;
    if rl == 0 {
      try!(r.read_end(c));
      if first {
       try!(r.read_header(c));
       first = false;
       rl = size_second;
      }
    };
  }
  assert!(&res[..] == expect);
  Ok(())
}

fn test_comp_one<T : ExtWrite + ExtRead>(
mut w : T,
mut r : T,
buf_len : usize,
input : &[&[u8];4],
expect : &[u8],
) -> Result<()> {
  let size_first = input[0].len() + input[1].len();
  let size_second = input[2].len() + input[3].len();
  let mut cout = Cursor::new(Vec::new());
  {
  let state = {
    let mut compw = CompW::new(&mut cout,&mut w);
    assert!(input[0].len() == try!(compw.write(input[0])));
    try!(compw.flush());
    assert!(input[1].len() == try!(compw.write(input[1])));
    try!(compw.write_end());
    try!(compw.flush()); 
    try!(CompW::suspend(compw))
  };
  try!(cout.write(&[123]));
  let mut compw = CompW::resume(&mut cout, state);
  assert!(input[2].len() == try!(compw.write(input[2])));
  assert!(input[3].len() == try!(compw.write(input[3])));
//  try!(compw.write_end());
 // try!(compw.flush());
  }
  let mut cin = cout;
  cin.set_position(0);

  let mut rl = size_first;
  let mut l;
  let mut res = Vec::new();
  let mut vbuf = vec![0;buf_len];
  let buf = &mut vbuf[..];
  let state = {
    let mut compr = CompR::new(&mut cin, &mut r);
    while {
      l = if rl == 0 {
        0
      } else if rl < buf.len() {
        try!(compr.read(&mut buf[..rl]))
      } else {
        try!(compr.read(buf))
      };
      l != 0
    } {
      res.extend_from_slice(&buf[..l]);
      rl = rl - l;
    };
    try!(compr.suspend())
  };
  let mut compr = {
    try!(cin.read(&mut buf[..1]));
    assert!(123 == buf[0]);
    CompR::resume(&mut cin, state)
  };
  rl = size_second;
  while {
    l = if rl == 0 {
      0
    } else if rl < buf.len() {
      try!(compr.read(&mut buf[..rl]))
    } else {
      try!(compr.read(buf))
    };
    l != 0
  } {
    res.extend_from_slice(&buf[..l]);
    rl = rl - l;
  }
  try!(compr.read_end()); // looking for new header

  assert!(&res[..] == expect);
  Ok(())
}

/// test using end extread and extwrite 
pub fn test_end_write<'a,'b,
  W : 'b + Write,
  EW : ExtWrite,
>(inp_length : usize, buf_length : usize, w : &mut W, ew : &mut EW) -> Result<Vec<u8>> {
  let mut rng = thread_rng();
  let mut write = CompW::new(w, ew);
  let mut reference = Cursor::new(Vec::new());
  let mut bufb = vec![0;buf_length];
  let buf = &mut bufb;

  let mut i = 0;
  while inp_length > i {
    rng.fill_bytes(buf);
    let ww = if inp_length - i < buf.len() {
      try!(reference.write(&buf[..inp_length - i]));
      try!(write.write(&buf[..inp_length - i])) 
    } else {
      try!(reference.write(buf));
      try!(write.write(buf))
    };
    assert!(ww != 0);
    i += ww;
  }
  println!("first");
  try!(write.write_end());
  // write next content : allways same for check of next ok
  
  buf[0] = 123;
  try!(write.write(&buf[..1]));
  println!("befflush");
  write.flush().unwrap();

  Ok(reference.into_inner())
}

/// test using end extread and extwrite 
/// endkind indicate if read can stop at end (for example endstream)
pub fn test_end_read<'a,'b,
  R : 'a + Read,
  ER : ExtRead,
>(reference : &[u8], inp_length : usize, buf_length : usize, r : &mut R, er : &mut ER, endkind : bool) -> Result<()> {
  let mut bufb = vec![0;buf_length];
  let buf = &mut bufb;

 
  let mut rr = 1;
  let mut i = 0;
  let mut bwr = CompR::new(r, er);
  while rr != 0 {
    if i + buf.len() > inp_length && !endkind {
      // read exact

      rr = try!(bwr.read(&mut buf[..inp_length - i]));
    } else {
      rr = try!(bwr.read(buf));
    }
    // it could go over reference as no knowledge (if padding)
    // inp length is here only to content assertion
    if rr != 0 {
    if i + rr > inp_length {
      if inp_length > i {
        let padstart = inp_length - i;
        assert!(buf[..padstart] == reference[i..]);
      } // else the window is bigger than buffer and padding is being read
    } else {
      assert!(buf[..rr] == reference[i..i + rr]);
    }
    }
    i += rr;
  }
  try!(bwr.read_end());
  assert!(i >= inp_length);
  
//  if endkind {
  let ni = try!(bwr.read(buf));
  assert!(ni >= 1);
  assert!(123 == buf[0]);

 // }
 
  Ok(())

}


#[test]
fn test_void_enr () {
  let mut inner = Cursor::new(Vec::new());
  let mut vw = Void;
  let mut vr = Void;
  let reference = test_end_write
    (123, 32, &mut inner, &mut vw).unwrap();
// inplength, buf length, write, extw 
  inner.set_position(0);
  test_end_read
    (&reference[..],
     123, 23, &mut inner, &mut vr, false).unwrap();
}

#[test]
fn test_void () {
  test_extwr(Void, Void,
  2,
  &[&vec![1,2,3],&vec![4,5],&vec![6,7,8],&vec![9]],
  &[1,2,3,4,5,6,7,8,9]
  ).unwrap();
  test_comp_one(Void, Void,
  2,
  &[&vec![1,2,3],&vec![4,5],&vec![6,7,8],&vec![9]],
  &[1,2,3,4,5,6,7,8,9]
  ).unwrap();

}





/// As a type alias no need to have R an W variant (no type constraint on type alias yet)
type CVoid<'a,'b,A> = CompW<'a,'b,A,Void>;
/// compose of EndStream over ciph
/// As a type alias no need to have R an W variant (no type constraint on type alias yet)
/// To use End read_end fonctionality it needs to be an outer one (otherwhise read end could not be
/// signaled).
type EndCiph<'a,'b,A> = CEndStream<'a,'b,CCiph<'b,'b,A>>;

/// EndCiph + Void for three layer
type EndCiphVoid<'a,'b,A> = EndCiph<'a,'b,CVoid<'b,'b,A>>;

type Cuvec = Cursor<Vec<u8>>;

fn checktype1 (_ : &CVoid<Cuvec>) {
}

fn checktype2 (_ : &CEndStream<CCiph<Cuvec>>) {
}
#[test]
fn test_suspend() {
  let mut w : Cuvec = Cursor::new(Vec::new());
  let mut v = Void;
  let state = {
  let mut void : CVoid<Cuvec> = CompW::new(&mut w, &mut v);
  checktype1(&void);
  void.write(&[0]).unwrap();
  void.suspend().unwrap()
  };
  w.write(&[0]).unwrap();
  let mut void : CVoid<Cuvec> = CompW::resume(&mut w, state);
  void.write(&[0]).unwrap();
}

#[test]
fn test_suspend2() {
  let mut w : Cuvec = Cursor::new(Vec::new());
  let mut c = Ciph::new(1,1);
  let mut e = EndStream::new(1);
  let (state,statein) = {
    let mut cyph : CCiph<Cuvec> = CompW::new(&mut w, &mut c);
    let state = {
      let mut endcyph : CEndStream<CCiph<Cuvec>> = CompW::new(&mut cyph, &mut e);
      checktype2(&endcyph);
      endcyph.write(&[0]).unwrap();
      endcyph.suspend().unwrap()
    };
    cyph.write(&[0]).unwrap();
    let state2 = {
    let mut endcyph : CEndStream<CCiph<Cuvec>> = CompW::resume(&mut cyph, state);

    // here check the type

    endcyph.write(&[0]).unwrap();
    endcyph.suspend().unwrap()
    };
    (cyph.suspend().unwrap(),state2)
  };
  w.write(&[0]).unwrap();
  let mut cyph : CCiph<Cuvec> = CompW::resume(&mut w, state);
  cyph.write(&[0]).unwrap();
  let mut endcyph : CEndStream<CCiph<Cuvec>> = CompW::resume(&mut cyph, statein);
  endcyph.write(&[0]).unwrap();
  //let mut void = CompW::resume(&mut w, state);
  //void.write(&[0]);
}

#[test]
fn test_ciph() {
  let towrite_size = 123;
  let ciphbuf = 7;
  let mut inner = Cursor::new(Vec::new());
  let reference = {
  let mut c = Ciph::new(3,ciphbuf);
  test_end_write
    (towrite_size, 32, &mut inner, &mut c).unwrap()
  };
  
  inner.set_position(0);
  let mut cr = Ciph::new(0,ciphbuf);
 
  test_end_read
    (&reference[..],
     towrite_size, 23, &mut inner, &mut cr, false).unwrap();
}

#[test]
fn test_endstream() {
  let towrite_size = 123;
  let mut inner = Cursor::new(Vec::new());
  let reference = {
  let mut c = EndStream::new(15);
  test_end_write
    (towrite_size, 32, &mut inner, &mut c).unwrap()
  };
  
  inner.set_position(0);
  let mut cr = EndStream::new(15);
 
  test_end_read
    (&reference[..],
     towrite_size, 23, &mut inner, &mut cr, true).unwrap();

}


#[test]
fn test_enstremciph() {
  let towrite_size = 23;
  let window_size = 15;
  let ciphbuf = 7;
  let mut inner = Cursor::new(Vec::new());
  let reference = {
  let mut c = Ciph::new(3,ciphbuf);
  let mut e = EndStream::new(window_size);
  let mut cyphw : CCiph<Cuvec> = CompW::new(&mut inner, &mut c);
  test_end_write
    (towrite_size, 32, &mut cyphw, &mut e).unwrap()
  };
  
  inner.set_position(0);
  let mut cr = Ciph::new(0,ciphbuf);
  let mut er = EndStream::new(window_size);
  let mut cyphr = CompR::new(&mut inner, &mut cr);
 
  test_end_read
    (&reference[..],
     towrite_size, 23, &mut cyphr, &mut er, true).unwrap(); // true for outer endstream

}

/// as EndStream is blocking for read and Ciph is not, and both write end content, 
/// using CompExtW<Ciph, EndStream> is not possible : ciph will write end before endstream blocking
/// end, then when reading ciph end will be ignored and after ublocking will be queried.
/// This would have been possible if ciph end method was blocking or if ciph does not write end
/// (true for most cipher where flush add the padding).
fn inst_ciph_end_mult () -> (Vec<CompExtW<EndStream, Ciph>>, Vec<CompExtR<EndStream, Ciph>>) {
  let c1 = Ciph::new_with_endval(1,3,4);
  let c2 = Ciph::new_with_endval(2,2,5);
  let c3 = Ciph::new_with_endval(3,5,6);
  let e1 = EndStream::new(5);
  let e2 = EndStream::new(3);
  let e3 = EndStream::new(4);
  (
  //vec![CompExtW(c3.clone(),e3.clone()),CompExtW(c2.clone(),e2.clone()),CompExtW(c1.clone(),e1.clone())],
  //vec![CompExtR(c3,e3),CompExtR(c2,e2),CompExtR(c1,e1)]
  vec![CompExtW(e3.clone(),c3.clone()),CompExtW(e2.clone(),c2.clone()),CompExtW(e1.clone(),c1.clone())],
  vec![CompExtR(e3,c3),CompExtR(e2,c2),CompExtR(e1,c1)]
  )
}

#[test]
fn test_ciph_end_mult () {
  let (ciphs,ciphsr) = inst_ciph_end_mult ();
  let mut w = Cursor::new(Vec::new());
  { // write end in drop
    let mut mciphsext = MultiWExt::new(ciphs);
    let mut mciphs = new_multiw(&mut w, &mut mciphsext);

    mciphs.write(&[123]).unwrap();
    mciphs.write_end().unwrap();
;
    mciphs.write(&[25]).unwrap();
;
  };
  //println!("debug mciphs {:?}",w.get_ref());
//[123, 0, 1, 0, 0, 1, 0, 0, 0, 25, 0, 1, 0, 0, 1, 0, 0, 0]

 
  // bigger buf than content
  let mut buf = vec![0;w.get_ref().len() + 10];
 
  let mut w = Cursor::new(w.into_inner());
  { 
    let mut mciphsext = MultiRExt::new(ciphsr);
    let mut mciphs = new_multir(&mut w, &mut mciphsext);
    let mut  r = mciphs.read(&mut buf[..]).unwrap();
    assert!(buf[0] == 123);
    // consume all kind of padding
    while r != 0 {
      let or = mciphs.read(&mut buf[..]);
      if !or.is_ok() {
        mciphs.2 = CompRState::Initial // avoid double panick TODO bug??
      }
      assert!(or.is_ok(), "Error : {:?}",or);
      r = or.unwrap();
    }

    //assert!(mciphs.read(&mut buf[..]).unwrap() == 0);
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


