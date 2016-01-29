//! 
//! This library purpose is to stack writers (and subsequently read with similar stack).
//!
//! This allow composition of writers.
//!
//! The library uses std::io::write and std::io::read for basis, but semantically Write and Read 
//! implementation requires a bit of care (see examples).
//!
//! Write implementation should write
//! all at once (all my test case run with this asumption but this lib code should run fine if
//! not).
//! Write ext trait is using header and an header is write again between each flush. Semantically
//! flush got extended meaning and consequently should not be called anywhere. 
//! For instance when
//! chaining two Write flush of the next one is never call when the first one flushed (this way we
//! could safely use it as a writer and flush after use. Chain flush is still use for MCompW
//! but only until the last writer and MCompR could use end_read symetrically (which CompR cannot
//! do (unless if decomposed : but out of its write/read trait context)).
//! 
//! Read implementation should retun the read size, and if read return 0 it means that the reading
//! is finished. (read ext trait should finalize the read (waiting for a new header)).
//!
//!
//! Semantically the library add 
//!
//!
#![feature(slice_bytes)] // TODO deprecated from 1.6

#[cfg(test)]
mod test;

use std::io::{
  Write,
  Read,
  Result,
};
use std::ops::Drop;

/// Write with further common functionnalities.
/// 
/// Compose over another Writer
/// TODO eg crypter
pub trait ExtWrite<W : Write> {

  /// write header if needed
  fn write_header(&mut self, &mut W) -> Result<()>;

  /// write buffer.
  fn write_into(&mut self, &mut W, &[u8]) -> Result<usize>;

  /// Could add end content (padding...) only if read can manage it
  #[inline]
  fn flush_into(&mut self, w : &mut W) -> Result<()> {w.flush()}

  /// write content at the end of stream. Read will be able to read it with a call to read_end.
  /// To use in a pure read write context, this is call on CompW Drop and should generally not need to be called manually.
  /// When the outer element of composition is removed drop finalize its action.
  /// TODO currently not called by flush as read got no symetric function
  fn write_end(&mut self, &mut W) -> Result<()>;

}

/// Compose over a reader with additional possibility to read an end content
pub trait ExtRead<R : Read> {

  /// read header (to initiate internal state) if needed
  fn read_header(&mut self, &mut R) -> Result<()>;

  /// read in buffer.
  fn read_from(&mut self, &mut R, &mut[u8]) -> Result<usize>;

  /// read end bytes (and possibly update internal state).
  /// To use in a pure read write context, this is call on CompR Drop and should generally not need to be called manually.
  /// When the outer element of composition is removed drop finalize its action.
  fn read_end(&mut self, &mut R) -> Result<()>;

}


#[derive(Clone)]
pub enum CompWState {
  /// new chain or after a flush, the head need to be written
  Initial,
  /// the head has been written we can write directly
  HeadWritten,
 /* /// Write end on next flush
  WriteEnd, // manually set state to read the end */
}

#[derive(Clone)]
pub enum CompRState {
  /// new read or after a read end, the head need to be read
  Initial,
  /// read has been initialized from head
  HeadRead,
/*  /// read need to read an end content before reading next head
  ReadEnd, // manually set state to read the end */
}


/// Base construct to build a write upon another one (composable writer).
///
/// This is used in many place with short lifecycle, it would be interesting to evaluate the 
/// overhead (or see if it is optimized).
pub struct CompW<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite<W>>(&'a mut W, &'b mut EW, CompWState);

/// Base construct to build a read upon another one (composable reader).
pub struct CompR<'a, 'b, R : 'a + Read, ER : 'b + ExtRead<R>>(&'a mut R, &'b mut ER, CompRState);


/// drop finalize but without catching possible issue TODO error mgmt?? include logger ?
impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite<W>> Drop for CompW<'a,'b,W,EW> {
  fn drop(&mut self) {
    self.write_end();
  }
}

/// drop finalize but without catching possible issue TODO error mgmt?? include logger ?
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead<R>> Drop for CompR<'a,'b,R,ER> {
  fn drop(&mut self) {
    self.read_end();
  }
}


impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite<W>> CompW<'a,'b,W,EW> {

  #[inline]
  pub fn new(w : &'a mut W, ew : &'b mut EW) -> Self {
    CompW(w,ew,CompWState::Initial)
  }

  #[inline]
  /// suspend write (inner writer is available again) but keep reference for subsequent write in same state
  pub fn suspend(self) -> Result<(&'b mut EW, CompWState)> {
    // manually to catch error instead of drop
    try!(self.1.write_end(self.0));
    Ok((self.1,self.2.clone()))
  }

  #[inline]
  pub fn resume(with : &'a mut W, from : (&'b mut EW, CompWState)) -> Self {
    CompW(with, from.0, from.1)
  }

  #[inline]
  fn write_end(&mut self) -> Result<()> {
    if let CompWState::HeadWritten = self.2 {
      try!(self.1.write_end(self.0));
      self.2 = CompWState::Initial;
    }
    Ok(())
  }
  /*pub fn set_end(&mut self) {
    self.2 = CompRState::WriteEnd
  }*/


}

impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead<R>> CompR<'a,'b,R,ER> {

  #[inline]
  pub fn new(r : &'a mut R, er : &'b mut ER) -> Self {
    CompR(r,er,CompRState::Initial)
  }

  #[inline]
  pub fn suspend(self) -> Result<(&'b mut ER, CompRState)> {
    // manually to catch error instead of drop
    try!(self.1.read_end(self.0));
    Ok((self.1,self.2.clone()))
  }

  #[inline]
  pub fn resume(with : &'a mut R, from : (&'b mut ER, CompRState)) -> Self {
    CompR(with, from.0, from.1)
  }

  #[inline]
  /// as there is no flush in read read end will be called out of Read interface
  pub fn read_end(&mut self) -> Result<()> {
    if let CompRState::HeadRead = self.2 {
      try!(self.1.read_end(self.0));
      self.2 = CompRState::Initial;
    }
    Ok(())
  }
/*
  /// we know that read is end but for any reason we could not call read_end
  /// so we flag as read end and subsequent read will only read end content
  /// (returning 0).
  /// Warning set_end involve that next read will be done. (with read_end the internal read should
  /// be reuse in different configuration)
  pub fn set_end(&mut self) {
    self.2 = CompRState::ReadEnd
  }
*/

}


impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite<W>> Write for CompW<'a,'b,W,EW> {
  fn write(&mut self, cont: &[u8]) -> Result<usize> {
    match self.2 {
      CompWState::Initial => {
        try!(self.1.write_header(self.0));
        self.2 = CompWState::HeadWritten;
      },
      CompWState::HeadWritten => (),
    };
    self.1.write_into(self.0, cont)
  }
  fn flush(&mut self) -> Result<()> {
    self.1.flush_into(self.0)
  }
}


/// TODO find a way to incorporate read end in Read of compR (for the moment you need to at least
/// use set_end (CompR fn). On Read_0 : no. TODO study other read functions.
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead<R>> Read for CompR<'a,'b,R,ER> {
  fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
    match self.2 {
      CompRState::Initial => {
          try!(self.1.read_header(self.0));
          self.2 = CompRState::HeadRead;
      },
      CompRState::HeadRead => (),
/*      CompRState::ReadEnd => {
          try!(self.1.read_end(self.0));
          try!(self.1.read_header(self.0));
          self.2 = CompRState::HeadRead;
      },*/
    };
    self.1.read_from(self.0, buf)
  }
}
 

/*
struct StreamShadow<'a, 'b, T : 'a + WriteTransportStream, S : 'b + Shadow>
(&'a mut T, &'b mut S, <S as Shadow>::ShadowMode);

struct ReadStreamShadow<'a, 'b, T : 'a + ReadTransportStream, S : 'b + Shadow>
(&'a mut T, &'b mut S, <S as Shadow>::ShadowMode);


impl<'a, 'b, T : 'a + WriteTransportStream, S : 'b + Shadow> Write for StreamShadow<'a,'b,T,S> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
      self.1.shadow_iter (buf, self.0, &self.2)
    }
    fn flush(&mut self) -> IoResult<()> {
      try!(self.1.shadow_flush(self.0, &self.2));
      self.0.flush()
    }
}

impl<'a, 'b, T : 'a + ReadTransportStream, S : 'b + Shadow> Read for ReadStreamShadow<'a,'b,T,S> {

  fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
    self.1.read_shadow_iter(self.0, buf, &self.2)
  }
}
*/

