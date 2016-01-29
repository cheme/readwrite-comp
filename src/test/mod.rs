//! Integration tests

use super::{
  ExtWrite,
  ExtRead,
  CompWState,
  CompRState,
  CompW,
  CompR,
};

use std::io::{
  Write,
  Read,
  Cursor,
  Result,
  Error,
  ErrorKind,
};

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

impl<W : Write> ExtWrite<W> for Void {
  #[inline]
  fn write_header(&mut self, _ : &mut W) -> Result<()> {Ok(())}
  #[inline]
  fn write_into(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {w.write(cont)}
  #[inline]
  fn write_end(&mut self, _ : &mut W) -> Result<()> {Ok(())}
}
impl<R : Read> ExtRead<R> for Void {
  #[inline]
  fn read_header(&mut self, _ : &mut R) -> Result<()> {Ok(())}
  #[inline]
  fn read_from(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    r.read(buf)
  }
  #[inline]
  fn read_end(&mut self, _ : &mut R) -> Result<()> {Ok(())}

}


fn test_extwr<T : ExtWrite<Cursor<Vec<u8>>> + ExtRead<Cursor<Vec<u8>>>>(
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
    println!("{:?}", cin.get_ref());
  cin.set_position(0);
  let c = &mut cin;
  let mut res = Vec::new();
  try!(r.read_header(c));
  let mut rl = size_first;
  let mut l = 0;
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

fn test_comp_one<T : ExtWrite<Cursor<Vec<u8>>> + ExtRead<Cursor<Vec<u8>>>>(
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
//  let state = {
    let mut compw = CompW::new(&mut cout,&mut w);
    assert!(input[0].len() == try!(compw.write(input[0])));
    try!(compw.flush());
    assert!(input[1].len() == try!(compw.write(input[1])));
  try!(compw.write_end());
 try!(compw.flush());
//    try!(CompW::suspend(compw))
//  };
  //try!(cout.write(&[123]));
 // let mut compw = CompW::resume(state, &mut cout);
  assert!(input[2].len() == try!(compw.write(input[2])));
  assert!(input[3].len() == try!(compw.write(input[3])));
//  try!(compw.write_end());
 // try!(compw.flush());
  }
  let mut cin = cout;
    println!("{:?}", cin.get_ref());
  cin.set_position(0);
  let mut compr = CompR::new(&mut cin, &mut r);
  let mut res = Vec::new();
  let mut rl = size_first;
  let mut l = 0;
  let mut vbuf = vec![0;buf_len];
  let mut first = true;
  let buf = &mut vbuf[..];
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
    if rl == 0 {
      if first {
     //   compr.set_end(); // looking for new header
        first = false;
        rl = size_second;
      };
      try!(compr.read_end()); // looking for new header
      
    };
  }
  assert!(&res[..] == expect);
  Ok(())
}
/*
pub fn test_end<'a,'b,
  R : 'a + Read,
  W : 'b + Write,
  ER : 'a + ExtRead<'a,R>,
  EW : 'b + ExtWrite<'b,W>,
>(inp_length : usize, buf_length : usize, w : &mut EW, r : &mut ER) -> Result<()> {
  let mut rng = OsRng::new().unwrap();
  let mut outputb = Cursor::new(Vec::new());
  let mut reference = Cursor::new(Vec::new());
  let output = &mut outputb;
  let mut bufb = vec![0;buf_length];
  let buf = &mut bufb;
  // knwoledge of size to write
  let mut i = 0;
  while inp_length > i {
    rng.fill_bytes(buf);
    if !bwr.has_started() {
      try!(bwr.start_write(output));
    };
    let ww = if inp_length - i < buf.len() {
      try!(reference.write(&buf[..inp_length - i]));
      try!(bwr.b_write(output,&buf[..inp_length - i])) 
    } else {
      try!(reference.write(buf));
      try!(bwr.b_write(output,buf))
    };
    assert!(ww != 0);
    i += ww;
  }
  try!(bwr.end_write(output));
  // write next content
  rng.fill_bytes(buf);
  let endcontent = buf[0];
  println!("EndContent{}",endcontent);
  try!(output.write(&buf[..1]));
  output.flush();

println!("Written lenght : {}", output.get_ref().len());
  // no knowledge of size to read
  output.set_position(0);
  let input = output;
  let mut rr = 1;
  i = 0;
  while rr != 0 {
    rr = try!(bwr.b_read(input,buf));
    // it could go over reference as no knowledge (if padding)
    // inp length is here only to content assertion
    println!("i {} rr {} inpl {}",i,rr,inp_length);
    if rr != 0 {
    if i + rr > inp_length {
      if inp_length > i {
        let padstart = inp_length - i;
        println !("pad start {}",padstart);
        assert!(buf[..padstart] == reference.get_ref()[i..]);
      } // else the window is bigger than buffer and padding is being read
    } else {
      assert!(buf[..rr] == reference.get_ref()[i..i + rr]);
    }
    }
    i += rr;
  }
println!("C");
  try!(bwr.end_read(input));
  assert!(i >= inp_length);
println!("D");
  let ni = try!(input.read(buf));
  assert!(ni == 1);
  assert!(endcontent == buf[0]);
  Ok(())

}*/



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
type CVoid<'a,'b,A> = CompW<'a,'b,Void,A>;
/// compose of EndStream over ciph
/// As a type alias no need to have R an W variant (no type constraint on type alias yet)
/// To use End read_end fonctionality it needs to be an outer one (otherwhise read end could not be
/// signaled).
type EndCiph<'a,'b,A> = CEndStream<'a,'b,CCiph<'b,'b,A>>;

/// EndCiph + Void for three layer
type EndCiphVoid<'a,'b,A> = EndCiph<'a,'b,CVoid<'b,'b,A>>;

#[test]
fn test_suspend() {
  let mut w = Cursor::new(Vec::new());
  let mut v = Void;
  let state = {
  let mut void = CompW::new(&mut w, &mut v);
  void.write(&[0]);
  void.suspend().unwrap()
  };
  w.write(&[0]);
  let mut void = CompW::resume(&mut w, state);
  void.write(&[0]);
}
/*
#[test]
fn test_suspend2() {
  let mut w = Cursor::new(Vec::new());
  let mut c = Ciph::new(1,1);
  let mut e = EndStream::new(1);
  let state = {
  let mut ECV = CompW::new(&mut CompW::new(&mut w, &mut v));
  void.write(&[0]);
  void.suspend().unwrap()
  };
  w.write(&[0]);
  let mut void = CompW::resume(&mut w, state);
  void.write(&[0]);
}
*/
