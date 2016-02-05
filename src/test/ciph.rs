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


use std::num::Wrapping;
use std::slice::bytes::copy_memory;


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
pub struct Ciph(u8, Vec<u8>, usize); 

pub type CCiph<'a,'b,A> = CompW<'a,'b,A,Ciph>;

impl Ciph {
  pub fn new(shift : u8, bufsize : usize) -> Self { 
    Ciph(shift, vec![0;bufsize], 0) 
  }
}



impl<W : Write> ExtWrite<W> for Ciph {

  #[inline]
  fn write_header(&mut self, w : &mut W) -> Result<()> {
    try!(w.write(&[self.0]));
    Ok(())
  }
  #[inline]
  fn write_into(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
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
        println!("writea : {:?}", &self.1[..]);
          tow -= try!(w.write(&self.1[..]));
        }
        self.2 = 0;
      }
    }
    Ok(tot)
  }

  #[inline]
  fn write_end(&mut self, w : &mut W) -> Result<()> {
    println!("In ciph write_end {}", self.2);
    if self.2 == 0 {

        println!("write6");
    try!(w.write(&[6]));
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
    try!(w.write(&[6]));

    Ok(())
  }
}

impl<R : Read> ExtRead<R> for Ciph {
  #[inline]
  fn read_header(&mut self, r : &mut R) -> Result<()> {
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
  fn read_from(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
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
           return Err(Error::new(ErrorKind::Other, "No bytes (encode should have written block buffer multiple content"));
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
  fn read_end(&mut self, r : &mut R) -> Result<()> {
    println!("In ciph read_end {}", self.2);
    let buf = &mut [9];
        println!("readend");
    let l = try!(r.read(buf));
    assert!(l==1);
    println!("{}",buf[0]);
    assert!(buf[0]==6);
 
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



