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
  CompR,
  CompExtW,
  CompExtR,
  MultiW,
  MultiR,
};
use super::{
  test_extwr,
  test_comp_one,
};


use std::num::Wrapping;
use std::slice::bytes::copy_memory;

#[test]
/// test on content with 3 layer (assert header and end of content are in the right place.
/// Plus variable buffer size
fn test_multiciph_w () {
  let c1 = Ciph::new_with_endval(1,3,4);
  let c2 = Ciph::new_with_endval(2,2,5);
  let c3 = Ciph::new_with_endval(3,5,6);
  let mut ciphs = [c3,c2,c1];
  let mut w = Cursor::new(Vec::new());
  { // write end in drop
    let mut mciphs = MultiW::new(&mut w, &mut ciphs);
    println!("actual write");
    mciphs.write(&[123]);
  };
  println!("debug mciphs {:?}",w.get_ref());
  // [1, 3, 6, 129, 6, 6, 6, 6, 9, 8, 6, 10, 9, 4]
  // (2pad and 6)
  // heads and val (shift1, shift2 + s1, shift3 + s2 + s1, val + s3 + s2+s1)
  assert!(&[1,3,6,129] == &w.get_ref()[..4]);
  // skip padding c3 (5 frame - 1 content) then end 3 + s2 + s1
  assert!(&[9] == &w.get_ref()[8..9]);
  // skip padding c2 (2 frame from 2 : 1 pad) then end 2 + s1
  assert!(&[6] == &w.get_ref()[10..11]);
  // skip padding c1 (3 frame from 1 : 2 pad) then end 1
  assert!(&[4] == &w.get_ref()[13..]);
}
#[test]
fn test_multiciph_r () {
  let c1 = Ciph::new_with_endval(1,3,4);
  let c2 = Ciph::new_with_endval(2,2,5);
  let c3 = Ciph::new_with_endval(3,5,6);
  let mut ciphs = [c3,c2,c1];
  let mut w = Cursor::new(vec!(1, 3, 6, 129, 6, 6, 6, 6, 9, 8, 6, 10, 9, 4));
  { 
    let mut mciphs = MultiR::new(&mut w, &mut ciphs);

    let mut buf = [0];
    mciphs.read(&mut buf[..]);
    assert!(buf[0] == 123);
    // manual read end to catch error
    assert!(mciphs.read_end().is_ok());
  };
  println!("debug mciphs {:?}",w.get_ref());
  // 
  // (2pad and 6)
  // heads and val (shift1, shift2 + s1, shift3 + s2 + s1, val + s3 + s2+s1)
  assert!(&[1,3,6,129] == &w.get_ref()[..4]);
  // skip padding c3 (5 frame - 1 content) then end 3 + s2 + s1
  assert!(&[9] == &w.get_ref()[8..9]);
  // skip padding c2 (2 frame from 2 : 1 pad) then end 2 + s1
  assert!(&[6] == &w.get_ref()[10..11]);
  // skip padding c1 (3 frame from 1 : 2 pad) then end 1
  assert!(&[4] == &w.get_ref()[13..]);
}


#[test]
/// test compw ordering of content
fn test_compciph_w () {
  let mut w = Cursor::new(Vec::new());
  { // write end in drop
    let mut c1 = Ciph::new_with_endval(1,3,4); // first pad will be 2 
    let mut c2 = Ciph::new_with_endval(2,2,5); // second pad will be one
    let mut cinner = CompW::new(&mut w,&mut c2);
    let mut comp = CompW::new(&mut cinner,&mut c1);
    comp.write(&[1]);
  };
  println!("debug mciphs {:?}",w.get_ref());
  //[2, 3, 4, 3, 3, 6, 5, 5]
  // heads and val
  assert!(&[2,3,4] == &w.get_ref()[..3]);
  // skip first padding 2 char (undefined value) and then end char c1 (+2)
  assert!(&[6] == &w.get_ref()[5..6]);
  // skip second padding 1 char (undefined value) and then end char c2
  assert!(&[5] == &w.get_ref()[7..]);
}
#[test]
/// test compw ordering of content
fn test_compciph_r () {
  let mut w = Cursor::new(vec![2, 3, 4, 3, 0, 6, 0, 5]);
//  let mut w = Cursor::new(vec![1,2,0,0,4]);
  let mut rr = 1;
  { 
    let mut c1 = Ciph::new_with_endval(1,3,4); 
    let mut c2 = Ciph::new_with_endval(2,2,5);
    let mut cinner = CompR::new(&mut w,&mut c2);
    let mut comp = CompR::new(&mut cinner,&mut c1);

    // read content of one byte (same as test_compciph_w
    let mut buf = [0];
    comp.read(&mut buf[..]);
    assert!(buf[0] == 1);
    // manual readend to catch errors
    assert!(comp.read_end().is_ok());
  };
}
#[test]
/// test compw ordering of content
fn test_compext_w () {
  let mut w = Cursor::new(Vec::new());
  { // write end in drop
    let mut c1 = Ciph::new_with_endval(1,3,4); // first pad will be 2 
    let mut c2 = Ciph::new_with_endval(2,2,5); // second pad will be one
    let mut compext = CompExtW(c1,c2);
    let mut comp = CompW::new(&mut w,&mut compext);
    comp.write(&[1]);
  };
  println!("debug mciphs {:?}",w.get_ref());
  //[2, 3, 4, 3, 3, 6, 5, 5]
  // heads and val
  assert!(&[2,3,4] == &w.get_ref()[..3]);
  // skip first padding 2 char (undefined value) and then end char c1 (+2)
  assert!(&[6] == &w.get_ref()[5..6]);
  // skip second padding 1 char (undefined value) and then end char c2
  assert!(&[5] == &w.get_ref()[7..]);
}
#[test]
/// test compw ordering of content
fn test_compext_r () {
  let mut w = Cursor::new(vec![2, 3, 4, 3, 0, 6, 0, 5]);
//  let mut w = Cursor::new(vec![1,2,0,0,4]);
  let mut rr = 1;
  {
    let mut c1 = Ciph::new_with_endval(1,3,4); // first pad will be 2 
    let mut c2 = Ciph::new_with_endval(2,2,5); // second pad will be one
    let mut compext = CompExtR(c1,c2);
    let mut comp = CompR::new(&mut w,&mut compext);

    // read content of one byte (same as test_compciph_w
    let mut buf = [0];
    comp.read(&mut buf[..]);
    assert!(buf[0] == 1);
    // manual readend to catch errors
    assert!(comp.read_end().is_ok());
  };
}









#[test]
fn test_ciph () {
  test_extwr(Ciph::new(2,2), Ciph::new(0,2),
  2,
  &[&vec![1,2,3],&vec![4,5],&vec![6,7,8],&vec![9]],
  &[1,2,3,4,5,6,7,8,9]
  ).unwrap();
  test_comp_one(Ciph::new(9,1), Ciph::new(0,1),
  3,
  &[&vec![1,2,3],&vec![4,5],&vec![6,7,8],&vec![9]],
  &[1,2,3,4,5,6,7,8,9]
  ).unwrap();

}



/// similar to a reader/writer that encrypt content (symetric key in header, usage of internal
/// buffer of fix size).
/// It only shift byte.
/// First u8 is the number of shift.
/// Second its buffer and third write ix in buf
/// The implementation only shift when buffer is full and add 0 bit padding to finalize (read and
/// write buff must be of same size).
/// Last is end char (default to 6)
#[derive(Clone)]
pub struct Ciph(u8, Vec<u8>, usize, u8); 

pub type CCiph<'a,'b,A> = CompW<'a,'b,A,Ciph>;

impl Ciph {
  pub fn new(shift : u8, bufsize : usize) -> Self { 
    Ciph(shift, vec![0;bufsize], 0, 6) 
  }
  pub fn new_with_endval(shift : u8, bufsize : usize, endval : u8) -> Self { 
    Ciph(shift, vec![0;bufsize], 0, endval) 
  }

}



impl ExtWrite for Ciph {

  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    println!("write header : {}",self.0);
    try!(w.write(&[self.0]));
    Ok(())
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    println!("writeinto : {:?}, shift {}", cont, self.0);
    let mut tot = 0;
    while tot < cont.len() {
      let ibufsize = self.1.len() - self.2;
      let contsize = cont.len() - tot;
      // fill buf
      let l = if ibufsize <  contsize {
          copy_memory(&cont[tot..tot + ibufsize], &mut self.1[self.2..]);
          ibufsize
      } else {
          copy_memory(&cont[tot..], &mut self.1[self.2..]);
          cont.len() - tot
      };
      tot += l;
      self.2 += l;
      if self.2 == self.1.len() {
        // do encode buffer when full onl
        for i in self.1.iter_mut() {
          *i = shift_up(*i,self.0);
        }
        // forward enc buf
        let mut tow = self.1.len();
        while tow > 0 {
        println!("writea : {:?}, shift {}", &self.1[..], self.0);
          tow -= try!(w.write(&self.1[..]));
        }
        self.2 = 0;
      }
    }
    Ok(tot)
  }

  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
//    println!("In ciph write_end {}", self.2);
    if self.2 == 0 {

        println!("write6");
    try!(w.write(&[self.3]));
      return Ok(())
    }
    // write buffer (all buffer so end is padding)
    for i in self.1.iter_mut() {
      *i = shift_up(*i,self.0);
    }
    // forward enc buf
    let mut tow = self.1.len();
    while tow > 0 {
      tow -= try!(w.write(&self.1[..]));
    }
        println!("writeb : {:?}", &self.1[..]);
    self.2 = 0;

        println!("write6");
    // add data for test only
    try!(w.write(&[self.3]));

    Ok(())
  }
}

impl ExtRead for Ciph {
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    println!("in read heder");
    let buf = &mut [9];
    let l = try!(r.read(buf));
    if l != 1 {
      return Err(Error::new(ErrorKind::Other, "No next header"));
    }
    self.0 = buf[0];
        println!("readhead {}",buf[0]);
    self.2 = self.1.len();
    Ok(())
  }
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {

    if buf.len() == 0 {
      return Ok(0);
    }
    let mut tot = 0;
    if self.2 == self.1.len() {
      self.2 = 0;
      // need to read next block
      while self.2 < self.1.len() {
        let l = try!(r.read(&mut self.1[tot..]));
        println!("read {}",l);
        if l == 0 {
          if self.2 == 0 {
            // a multiple buf has been written but nothing could be read : we consider that it is
            // no error (for instance with end stream)
            return Ok(0);
          } else {
           return Err(Error::new(ErrorKind::Other, "No bytes (encode should have written block buffer multiple content"));
          }
        }
        tot += l;
        self.2 += l;
      }
      for i in self.1.iter_mut() {
        *i = shift_down(*i,self.0);
      }
      self.2 = 0;
    }; 
    // read from buffer
    let tocopy =  if buf.len() > self.1.len() - self.2 {
        self.1.len() - self.2
    } else {
        buf.len()
    };

    copy_memory(&self.1[self.2..self.2 + tocopy], &mut buf[..tocopy]);
    self.2 += tocopy;
    Ok(tocopy)
  }
  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    println!("In ciph read_end {}", self.2);
    let buf = &mut [9];
        println!("readend");
    let l = try!(r.read(buf));
    assert!(l==1);
    println!("{}",buf[0]);
    assert!(buf[0]==self.3);
 
    // put ix at start val (the end of read content must have been padding)
    self.2 = self.1.len();
    Ok(())
  }

}


#[inline]
fn shift_up(init : u8, inc : u8) -> u8 {
  (Wrapping(init) + Wrapping(inc)).0
}
#[inline]
fn shift_down(init : u8, dec : u8) -> u8 {
  (Wrapping(init) - Wrapping(dec)).0
}



