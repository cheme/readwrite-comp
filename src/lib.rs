//! 
//! This library purpose is to stack writers (and subsequently read with similar stack).
//!
//! This allow composition of writers.
//!
//! The library uses std::io::write and std::io::read for basis, but semantically Write and Read 
//! implementation requires a bit of care (see examples).
//!
//! Ext trait add a possible header writing/reading and a possible end of message writing/reading. 
//!
//! Read implementation should return the read size, and if read return 0 it means that the reading
//! is finished. (read ext trait should finalize the read (waiting for a new header)).
//!
//!
//! Most of this library is using WriteExt and ReadExt trait which allow to define additional
//! action over standard read and write for example :
//! - encyphering content : an additional header is required in most case.
//! - adding info to content : like control, for instance an end of frame byte (required an end of
//! message).
//! - linking two reader or two writer (for instance CompExtW do it)
//! The point is that WriteExt and ReadExt does not compose over the internal reader/writer to
//! allow things such as MultiW or MultiR where we got a final Writer or final Reader but an
//! undefined number of ExtWriter and ExtRead (and still static type without fat pointer).
//!
//! WriteExt and ReadExt could be composed, using MultiW/R or CopmExtW/R.
//!
//! WriteExt and ReadExt could be used as standard Reader or Writer by using CompW or CompR, 
//!
//! Composition by creating CompW of CompW as Writer and CompW as WriterExt is not really
//! encouraged (even if some test are included) due to difficulty to write header or end of message
//! recursivly (the first component is seen as a Read or a Write). CompW should in priority as a
//! last wrapper.
//! CompExtWInner and CompExtRInner are an alternative to CompW and CompR with less overhead but no
//! guaranties over header and end of message. (first they was private but prove usefull in some
//! cases).
//!
//!
//! Current issue with this crate are
//! - drop cause panic in panic when read_end / write_end panic : leading to no clue about the issue
//! - flush is not recursive : only the extWrite flush, the inner writer does not : TODO add a bool
//! in flush_into to say if inner writer must be flush : that way on drop inner writer will not be
//! flush (but still contain end_write), and have the composition flush flush all the way. Plus
//! expose in object (CompW, MultiComp) this boolean. CompExtW is already recursive by default and
//! should stay like that.
//! : inner writer must flush at each layer. (MultiW flush until the inner writer of course)
//! - flush and write_end semantic is tricky : flush means that no counterpart is needed in read
//!   (we can flush anywhere without a failure from read) whereas write_end involve a read_end need.
//!   Read_end is triggered manually with two cases :
//!   - we know the length to read (for instance a serialized object read from a reader) : that is
//!   easy and not a big issue
//!   - we do not know the length (for instance proxy of encrypted content) : then a reader like
//!   endstream could be used and read will block when seeing a end content by returning Ok(0)
//!   until read_end is used to unlock the reading. There is a serious limitition here when
//!   composing this kind of bloquing reader with non bloquing reader having some end content : the bloquing one must be in
//!   the internal layer, otherwhise the end sequence of the non bloquing will be skipped (this is 
//!   a somehow tricky case but also not so common (some cipher does not require end as using flush
//!   is enough (endstream may still be used for perf when proxying content that we do not want to
//!   read)).
//! - symetry between read and write is not enforced, non symetric implementation will fail
//!
//!
//! Example of usage could be found it tests, but also in mydht-base tunnel implementation,
//! mydht-base bytes_wr and
//! mydht shadow (for example mydht-openssl).
//!


#![cfg_attr(feature="with-clippy", feature(plugin))]

#![cfg_attr(feature="with-clippy", plugin(clippy))]


#[cfg(test)]
mod test;

#[cfg(test)]
extern crate rand;

use std::io::{
  Write,
  Read,
  Result,
  Error,
  ErrorKind,
};
use std::ops::Drop;
use std::slice::Iter;

use std::rc::Rc;
use std::cell::RefCell;
use std::cell::BorrowMutError;
/// Write with further common functionnalities.
/// 
/// Compose over another Writer
/// TODO eg crypter
pub trait ExtWrite {

  /// write header if needed
  fn write_header<W : Write>(&mut self, &mut W) -> Result<()>;

  /// write buffer.
  fn write_into<W : Write>(&mut self, &mut W, &[u8]) -> Result<usize>;

  /// write all
  fn write_all_into<W : Write>(&mut self, w : &mut W, mut buf : &[u8]) -> Result<()> {
    while !buf.is_empty() {
      match self.write_into(w, buf) {
        Ok(0) => return Err(Error::new(ErrorKind::WriteZero,
                    "failed to write whole buffer")),
        Ok(n) => buf = &buf[n..],
        Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
        Err(e) => return Err(e),
      }
    }
    Ok(())
  }

  /// Could add end content (padding...) only if read can manage it
  /// does not flush recursivly
  #[inline]
  fn flush_into<W : Write>(&mut self, _ : &mut W) -> Result<()> {Ok(())}

  /// write content at the end of stream. Read will be able to read it with a call to read_end.
  /// To use in a pure read write context, this is call on CompW Drop and should generally not need to be called manually.
  /// When the outer element of composition is removed drop finalize its action.
  /// TODO currently not called by flush as read got no symetric function
  fn write_end<W : Write>(&mut self, &mut W) -> Result<()>;

}

/* cannot as wetmp need to be instantiated out of new last for ref : see if as ref...
impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite<WETmp<'a,W>>> CompW2<'a,'b,WETmp<'a,W>,EW> {

  #[inline]
  pub fn new_last(w : &'a mut W, ew : &'b mut EW) -> Self {
    CompW2(WETmp(w),ew,CompWState::Initial)
  }
}*/

/// Compose over a reader with additional possibility to read an end content
pub trait ExtRead {

  /// read header (to initiate internal state) if needed
  fn read_header<R : Read>(&mut self, &mut R) -> Result<()>;

  /// read in buffer.
  fn read_from<R : Read>(&mut self, &mut R, &mut[u8]) -> Result<usize>;

  /// read exact
  fn read_exact_from<R : Read>(&mut self, r : &mut R, mut buf: &mut[u8]) -> Result<()> {
    while !buf.is_empty() {
      match self.read_from(r,buf) {
        Ok(0) => break,
        Ok(n) => { let tmp = buf; buf = &mut tmp[n..]; }
        Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
        Err(e) => return Err(e),
      }
    }
    if buf.is_empty() {
      Ok(())
    } else {
      Err(Error::new(ErrorKind::UnexpectedEof,
                  "failed to fill whole buffer"))
    }
  }

  /// read up to first no content read and apply read_end
  fn read_to_end<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<()> {
    while { self.read_from(r,buf)? != 0} {}
    self.read_end(r)
  }
  /// read end bytes (and possibly update internal state).
  /// To use in a pure read write context, this is call on CompR Drop and should generally not need to be called manually.
  /// When the outer element of composition is removed drop finalize its action.
  fn read_end<R : Read>(&mut self, &mut R) -> Result<()>;

  fn chain<'a, 'b, R : ExtRead + 'b>(&'a mut self, next : &'b mut R) -> ChainExtRead<'a,'b,Self,R> where Self: Sized + 'a {
        ChainExtRead { first: self, second: next, done_first: false, second_header_done : false }
  }
  fn chain_with_initialized<'a, 'b, R : ExtRead + 'b>(&'a mut self, next : &'b mut R) -> ChainExtRead<'a,'b,Self,R> where Self: Sized + 'a {
        ChainExtRead { first: self, second: next, done_first: false, second_header_done : true }
  }

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


/// Base construct to build a write upon another one (composable writer), using an ExtWrite
/// implementation.
///
/// This is used in many place with short lifecycle, it would be interesting to evaluate the 
/// overhead (or see if it is optimized).
///
/// When composing multiple ExtWrite, one could compose with multiple CompW , but as the first
/// parameter is simply Write, write end will not be called recursively (for usage as a standard
/// Write it is safe as write end will be called recursively through drop (yet with no catching of
/// the possible errors).
/// For composing with full support for write_end, consider only one layer of CompW and composing 
/// the ExtWrit with a CompExtW composition.
///
pub struct CompW<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite>(pub &'a mut W, pub &'b mut EW, pub CompWState);
//pub struct CompWOwn<'a, W : 'a + Write, EW : ExtWrite>(pub &'a mut W, pub EW, pub CompWState);

/// inner struct for implemention just to apply method of sw in write
/// This is not to be use directly as write but just to use write_into and flush_into
pub struct CompExtWInner<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite>(pub &'a mut W, pub &'b mut EW);

impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> Write for CompExtWInner<'a, 'b, W, EW> {
  #[inline]
  fn write(&mut self, cont: &[u8]) -> Result<usize> {
    self.1.write_into(self.0, cont)
  }
  #[inline]
  fn flush(&mut self) -> Result<()> {
    self.1.flush_into(self.0)
  }
  #[inline]
  fn write_all(&mut self, cont: &[u8]) -> Result<()> {
    self.1.write_all_into(self.0, cont)
  }
 
}

/// Compose two ExtWrite in a single on with Owned ExtWrite.
/// EW1 apply over EW2 meaning that EW2 is the external layer (ew2 header written first without
/// applying ew1 over it and ew2 end written last without ew1 written over it and content written
/// by first applying ew2 then ew1.
pub struct CompExtW<EW1 : ExtWrite, EW2 : ExtWrite>(pub EW1, pub EW2);

impl<EW1 : ExtWrite, EW2 : ExtWrite> ExtWrite for CompExtW<EW1, EW2> {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    try!(self.1.write_header(w));
    self.0.write_header(&mut CompExtWInner(w, &mut self.1))
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    self.0.write_into(&mut CompExtWInner(w, &mut self.1),cont)
  }
  #[inline]
  fn write_all_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<()> {
    self.0.write_all_into(&mut CompExtWInner(w, &mut self.1),cont)
  }
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {
    try!(self.0.flush_into(&mut CompExtWInner(w, &mut self.1)));
    self.1.flush_into(w)
  }
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    try!(self.0.write_end(&mut CompExtWInner(w, &mut self.1)));
    self.1.write_end(w)
  }
}


/// Base construct to build a read upon another one (composable reader).
pub struct CompR<'a, 'b, R : 'a + Read, ER : 'b + ExtRead>(pub &'a mut R, pub &'b mut ER, pub CompRState);


pub struct CompExtR<EW1 : ExtRead, EW2 : ExtRead>(pub EW1, pub EW2);
impl<EW1 : ExtRead, EW2 : ExtRead> ExtRead for CompExtR<EW1, EW2> {
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    try!(self.1.read_header(r));
    self.0.read_header(&mut CompExtRInner(r, &mut self.1))
  }

  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    self.0.read_from(&mut CompExtRInner(r, &mut self.1),buf)
  }

  #[inline]
  fn read_exact_from<R : Read>(&mut self, r : &mut R, mut buf: &mut[u8]) -> Result<()> {
    self.0.read_exact_from(&mut CompExtRInner(r, &mut self.1),buf)
  }

  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    try!(self.0.read_end(&mut CompExtRInner(r, &mut self.1)));
    self.1.read_end(r)
  }

}

/// Inner construct to build a read upon another one, do not use as write if you need automatic
/// header or automatic end (technical).
pub struct CompExtRInner<'a, 'b, R : 'a + Read, ER : 'b + ExtRead>(pub &'a mut R, pub &'b mut ER);

impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Read for CompExtRInner<'a,'b,R,ER> {
  #[inline]
  fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
    self.1.read_from(self.0, buf)
  }

  #[inline]
  fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
    self.1.read_exact_from(self.0, buf)
  }
}


// TODO non mandatory log dependancy and a version of this which log errors
#[inline]
fn result_in_drop(_ : Result<()>) {
}

/// drop finalize but without catching possible issue TODO error mgmt?? include logger ? Or upt to
/// ExtWrite write_end implementation to be safe (return allways ok and synch over shared variable
/// like arcmut or jus rc cell and have something else managing this error
/// Drop is only for reference CompW, for Own compw drop does not exists and should no be used as 
/// a Write.
impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> Drop for CompW<'a,'b,W,EW> {
  fn drop(&mut self) {
    if let CompWState::HeadWritten = self.2 {
      result_in_drop(self.write_end());
      result_in_drop(self.flush());
    }
  }
}


/// drop finalize but without catching possible issue TODO error mgmt?? include logger ?
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Drop for CompR<'a,'b,R,ER> {
  fn drop(&mut self) {
    if let CompRState::HeadRead = self.2 {
      result_in_drop(self.read_end());
    }
  }
}


impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> CompW<'a,'b,W,EW> {

  #[inline]
  pub fn new(w : &'a mut W, ew : &'b mut EW) -> Self {
    CompW(w,ew,CompWState::Initial)
  }

  #[inline]
  /// suspend write (inner writer is available again) but keep reference for subsequent write in same state
  pub fn suspend(mut self) -> Result<(&'b mut EW, CompWState)> {
    // manually to catch error instead of drop
    if let CompWState::HeadWritten = self.2 {
      try!(self.write_end());
      try!(self.flush());
      self.2 = CompWState::Initial;
    }
    Ok((self.1,self.2.clone()))
  }

  #[inline]
  pub fn resume(with : &'a mut W, from : (&'b mut EW, CompWState)) -> Self {
    CompW(with, from.0, from.1)
  }

  #[inline]
  pub fn write_end(&mut self) -> Result<()> {
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
/*
impl<'a, W : 'a + Write, EW : ExtWrite> CompWOwn<'a,W,EW> {
  #[inline]
  pub fn new(w : &'a mut W, ew : EW) -> Self {
    CompWOwn(w,ew,CompWState::Initial)
  }
  #[inline]
  /// suspend write (inner writer is available again) but keep reference for subsequent write in same state
  pub fn suspend(mut self) -> Result<(EW, CompWState)> {
    // manually to catch error instead of drop
    if let CompWState::HeadWritten = self.2 {
      try!(self.write_end());
      try!(self.flush());
      self.2 = CompWState::Initial;
    }
    Ok((self.1,self.2))
  }
  #[inline]
  pub fn resume(with : &'a mut W, from : (EW, CompWState)) -> Self {
    CompWOwn(with, from.0, from.1)
  }
  #[inline]
  pub fn write_end(&mut self) -> Result<()> {
    if let CompWState::HeadWritten = self.2 {
      try!(self.1.write_end(self.0));
      self.2 = CompWState::Initial;
    }
    Ok(())
  }
}*/

impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> CompR<'a,'b,R,ER> {

  #[inline]
  pub fn new(r : &'a mut R, er : &'b mut ER) -> Self {
    CompR(r,er,CompRState::Initial)
  }

  #[inline]
  pub fn suspend(mut self) -> Result<(&'b mut ER, CompRState)> {
    // manually to catch error instead of drop
    if let CompRState::HeadRead = self.2 {
      try!(self.1.read_end(self.0));
      self.2 = CompRState::Initial;
    }
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
macro_rules! write_impl_comp {() => (
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
)}

/// write_end support through drop only (should only be used as single write when 
/// write end error could be ignored).
impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> Write for CompW<'a,'b,W,EW> {
  write_impl_comp!();
}
/*
/// incomplet write : no write end support.
impl<'a, W : 'a + Write, EW : ExtWrite> Write for CompWOwn<'a,W,EW> {
  write_impl_comp!();
}*/

/// TODO find a way to incorporate read end in Read of compR (for the moment you need to at least
/// use set_end (CompR fn). On Read_0 : no. TODO study other read functions.
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Read for CompR<'a,'b,R,ER> {
  fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
    match self.2 {
      CompRState::Initial => {
          try!(self.1.read_header(self.0));
//    panic!("dd");
          self.2 = CompRState::HeadRead;
//    panic!("dd");
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



/// CompW with several (undefined number at compile time) Write of same kind to chain, and a dest
/// write.
/// Drop semantic for use as write cannot be enable (inner temporary use of MCompW would write_end
/// at each write).
/// This could be use for layered write (for example in a multilayer ssl tunnel).
///
/// Suspend (up to W) and write end will impact all layer.
///
/// Order of layer is external layer last (the writer is therefore logicaly at the end of the
/// array of layer).
///
pub type MultiW<'a, 'b, W, EW> = CompW<'a,'b,W,MultiWExt<EW>>;
//pub type MultiW<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> = CompW<'a,'b,W,MultiWExt<EW>>;

pub struct MultiWExt<EW : ExtWrite>(Vec<EW>, Vec<CompWState>);

/// TODO fn to remove one layer with ok write end (similar to suspend but with)
/// MCompW is using drop to write end (for write use).
/// It does not drop (it drop but drop does not writeend) when used internally by playing on
/// states.
/// For performance purpose an External and Internal struct could be defined latter (with and
/// without drop plus lighter imp for internal).
/// Hack and run internal MCompW with a InitState before drop and with a
/// headWritten state when we already write head (cf shadow &'a[bool])a : use state instead of bool
struct MCompW<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite>(&'a mut W, &'b mut[EW], &'b mut [CompWState]);


/// Multiple layered read (similar to MCompW).
pub type MultiR<'a, 'b, R, ER> = CompR<'a,'b,R,MultiRExt<ER>>;
//pub type MultiR<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> = CompR<'a,'b,R,MultiRExt<ER>>;

pub struct MultiRExt<ER : ExtRead>(Vec<ER>, Vec<CompRState>);

struct MCompR<'a, 'b, R : 'a + Read, ER : 'b + ExtRead>(&'a mut R, &'b mut[ER], &'b mut [CompRState]);

impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> MCompW<'a,'b,W,EW> {

  #[inline]
  fn write_header(&mut self) -> Result<()> {
    match self.2[0] {
      CompWState::Initial => {
        if self.1.len() > 1 {
          if let Some((f,last)) = self.1.split_first_mut() {
          let mut el = MCompW(self.0, last, &mut self.2[1..]); 
          try!(f.write_header(&mut el));
          try!(el.write_header());

        }} else {
          try!((self.1).get_mut(0).unwrap().write_header(self.0));
        };
        self.2[0] = CompWState::HeadWritten;
      },
      CompWState::HeadWritten => (),
    };
    Ok(())
  }

  #[inline]
  fn write_end(&mut self) -> Result<()> {
    match self.2[0] {
      CompWState::HeadWritten => {
 
        if self.1.len() > 1 {
        if let Some((f,last)) = self.1.split_first_mut()  {
          let mut el = MCompW(self.0, last, &mut self.2[1..]);
          try!(f.write_end(&mut el));
          try!(el.write_end());
        }
        } else {
          // last
          try!((self.1).get_mut(0).unwrap().write_end(self.0));
        };
        self.2[0] = CompWState::Initial;
        Ok(())

      },
      CompWState::Initial => Ok(()),
    }
  }

}

impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> MCompR<'a,'b,R,ER> {

  #[inline]
  /// as there is no flush in read read end will be called out of Read interface
  pub fn read_end(&mut self) -> Result<()> {
    match self.2[0] {
      CompRState::HeadRead => {
        if self.1.len() > 1 {
        if let Some((f,last)) = self.1.split_first_mut()  {
          let mut el = MCompR(self.0, last, &mut self.2[1..]);
          try!(f.read_end(&mut el));
          try!(el.read_end());
        }
        } else {
          // last
          try!((self.1).get_mut(0).unwrap().read_end(self.0));
        };
        self.2[0] = CompRState::Initial;
        Ok(())
      },
      CompRState::Initial => Ok(()),
    }
  }
  #[inline]
  fn read_header(&mut self) -> Result<()> {
    match self.2[0] {
      CompRState::Initial => {
        if self.1.len() > 1 {
          if let Some((f,last)) = self.1.split_first_mut() {
          let mut el = MCompR(self.0, last, &mut self.2[1..]); 
          try!(f.read_header(&mut el));
          try!(el.read_header());

        }} else {
          try!((self.1).get_mut(0).unwrap().read_header(self.0));
        };
        self.2[0] = CompRState::HeadRead;
      },
      CompRState::HeadRead => (),
    };
    Ok(())
  }
}

#[inline]
pub fn new_multiw<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> 
  (w : &'a mut W, ew : &'b mut MultiWExt<EW>) -> MultiW<'a,'b,W,EW> {
    CompW::new(w, ew)
}

#[inline]
pub fn new_multir<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> 
  (r : &'a mut R, er : &'b mut MultiRExt<ER>) -> MultiR<'a,'b,R,ER> {
    CompR::new(r, er)
}

/*
// TODO try a box new multiw (compw need to use as_ref))
pub fn new_multiw<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> 
  (w : &'a mut W, ew : &'b mut [EW]) -> MultiW<'a,'b,W,EW> {
    CompW::new(w, Box::new(MultiWExt::new(ew)))
}
*/
impl<EW : ExtWrite> MultiWExt<EW> {
  #[inline]
  pub fn inner_extwrites(&self) -> &[EW] {
    &self.0
  }
  #[inline]
  pub fn inner_extwrites_mut(&mut self) -> &mut [EW] {
    &mut self.0
  }
 
  #[inline]
  fn inner<'c,'b, W : Write>(&'c mut self, w : &'b mut W) -> MCompW<'b,'c,W,EW> {
    MCompW(w,&mut self.0[..],&mut self.1[..])
  }
  #[inline]
  pub fn new(ew : Vec<EW>) -> Self {
    let state = Self::init_state(&ew[..]);
    MultiWExt(ew,state)
  }

  #[inline]
  pub fn init_state(ew : &[EW]) -> Vec<CompWState> {
    vec![CompWState::Initial; ew.len()]
  }
}


impl<ER : ExtRead> MultiRExt<ER> {
  #[inline]
  pub fn len(&self) -> usize {
    self.0.len()
  }
  #[inline]
  pub fn iter(&self) -> Iter<ER> {
    self.0.iter()
  }
  #[inline]
  fn inner<'c,'b,R : Read>(&'c mut self, r : &'b mut R) -> MCompR<'b,'c,R,ER> {
    MCompR(r,&mut self.0[..],&mut self.1[..])
  }
  #[inline]
  pub fn new(ew : Vec<ER>) -> Self {
    let state = Self::init_state(&ew[..]);
    MultiRExt(ew,state)
  }
  #[inline]
  pub fn init_state(ew : &[ER]) -> Vec<CompRState> {
    vec![CompRState::Initial; ew.len()]
  }
}


impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> Write for MCompW<'a,'b,W,EW> {
  fn write(&mut self, cont: &[u8]) -> Result<usize> {
    try!(self.write_header());
    if self.1.len() > 1 {
      if let Some((f,last)) = self.1.split_first_mut() {
        let mut el = MCompW(self.0, last, &mut self.2[1..]); 
        return f.write_into(&mut el, cont);
      }
    }
    // last
    (self.1).get_mut(0).unwrap().write_into(self.0, cont)
 
  }

  /// flush all layer
  fn flush(&mut self) -> Result<()> {
    if self.1.len() > 1 {
    if let Some((f,last)) = self.1.split_first_mut()  {
      let mut el = MCompW(self.0, last, &mut self.2[1..]);
      return f.flush_into(&mut el);
    }
    }
    // last
    try!((self.1).get_mut(0).unwrap().flush_into(&mut self.0));
    self.0.flush()
  }
}
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Read for MCompR<'a,'b,R,ER> {
  fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
    try!(self.read_header());
    if self.1.len() > 1 {
      if let Some((f,last)) = self.1.split_first_mut() {
        let mut el = MCompR(self.0, last, &mut self.2[1..]); 
        return f.read_from(&mut el, buf);
      }
    }
    // last
    (self.1).get_mut(0).unwrap().read_from(self.0, buf)
  }
}

impl<EW : ExtWrite> ExtWrite for MultiWExt<EW> {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    self.inner(w).write_header()
  }
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    self.inner(w).write_end()
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont: &[u8]) -> Result<usize> {
    self.inner(w).write(cont)
  }
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {
    self.inner(w).flush()
  }
}


impl<EW : ExtRead> ExtRead for MultiRExt<EW> {
//impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Read for MultiR<'a,'b,R,ER> {
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf: &mut [u8]) -> Result<usize> {
    self.inner(r).read(buf)
  }
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    self.inner(r).read_header()
  }
  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    self.inner(r).read_end()
  }
}

/**
 * No op reader/writer : do not comp
 */
pub struct ID ();
impl ExtRead for ID {
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf: &mut [u8]) -> Result<usize> {
    r.read(buf)
  }
  #[inline]
  fn read_header<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn read_end<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn read_exact_from<R : Read>(&mut self, r : &mut R, mut buf: &mut[u8]) -> Result<()> {
    r.read_exact(buf)
  }
}


impl ExtWrite for ID {
  #[inline]
  fn write_header<W : Write>(&mut self, _ : &mut W) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn write_end<W : Write>(&mut self, _ : &mut W) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont: &[u8]) -> Result<usize> {
    w.write(cont)
  }
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {
    w.flush()
  }
  #[inline]
  fn write_all_into<W : Write>(&mut self, w : &mut W, buf : &[u8]) -> Result<()> {
    w.write_all(buf)
  }
}

pub struct BorrowMutErr(BorrowMutError);
impl From<BorrowMutErr> for Error {
  #[inline]
  fn from(e : BorrowMutErr) -> Error {
    Error::new(ErrorKind::Other, e.0)
  }
}

impl<ER : ExtRead> ExtRead for RefCell<ER> {
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_header(r)
  }

  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_from(r,buf)
  }

  #[inline]
  fn read_exact_from<R : Read>(&mut self, r : &mut R, mut buf: &mut[u8]) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_exact_from(r,buf)
  }

  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_end(r)
  }

}

impl<EW : ExtWrite> ExtWrite for RefCell<EW> {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_header(w)
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_into(w,cont)
  }
  #[inline]
  fn write_all_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_all_into(w,cont)
  }
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.flush_into(w)
  }
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_end(w)
  }
}

impl<ER : ExtRead> ExtRead for Rc<RefCell<ER>> {
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_header(r)
  }

  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf : &mut[u8]) -> Result<usize> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_from(r,buf)
  }

  #[inline]
  fn read_exact_from<R : Read>(&mut self, r : &mut R, mut buf: &mut[u8]) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_exact_from(r,buf)
  }

  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.read_end(r)
  }

}

impl<EW : ExtWrite> ExtWrite for Rc<RefCell<EW>> {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_header(w)
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<usize> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_into(w,cont)
  }
  #[inline]
  fn write_all_into<W : Write>(&mut self, w : &mut W, cont : &[u8]) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_all_into(w,cont)
  }
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.flush_into(w)
  }
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    let mut inner = try!(self.try_borrow_mut().map_err(|e|BorrowMutErr(e)));
    inner.write_end(w)
  }
}
/// Chain two extreader, read end of first and header of second (if needed) as soon as it read 0 length content
/// TODO test case!!
pub struct ChainExtRead<'a, 'b, T : ExtRead + 'a, U : ExtRead + 'b> {
    first: &'a mut T,
    second: &'b mut U,
    done_first: bool,
    second_header_done : bool,
}

impl<'a, 'b, T : ExtRead + 'a, U : ExtRead + 'b> ChainExtRead<'a,'b,T,U> {
  pub fn in_first (&self) -> bool { !self.done_first }
  pub fn in_second (&self) -> bool { self.done_first }
  #[inline]
  fn switch_to_second<R : Read> (&mut self, r : &mut R) -> Result<()> {

    self.first.read_end(r)?;
    self.done_first = true;
    if !self.second_header_done {
      self.second.read_header(r)?;
      // self.second_header_done = true;
    }
    Ok(())
  }
}

impl<'a, 'b, T : ExtRead + 'a, U : ExtRead + 'b> ExtRead for ChainExtRead<'a,'b,T,U> {
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    self.first.read_header(r)
  }

  fn read_from<R : Read>(&mut self, r : &mut R, buf: &mut[u8]) -> Result<usize> {
    if !self.done_first {
      let i = self.first.read_from(r,buf)?;
      if i == 0 {
        self.switch_to_second(r)?;
        self.second.read_from(r,buf)
      } else {
        Ok(i)
      }
    } else {
      self.second.read_from(r,buf)
    }
  }

  fn read_exact_from<R : Read>(&mut self, r : &mut R, buf: &mut[u8]) -> Result<()> {
    let mut i = 0;
    if !self.done_first {
      loop {
        let iit = self.first.read_from(r,&mut buf[i..])?;
        i += iit;
        if iit == 0 {
          self.switch_to_second(r)?;
          break;
        }
      }
      if i == buf.len() {
        return Ok(())
      }
    }
    self.second.read_exact_from(r,&mut buf[i..])
  }
  
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    if !self.done_first {
      self.switch_to_second(r)?
    }
    self.second.read_end(r)?;
    // reinit reader (costless)
    self.done_first = false;
    self.second_header_done = false;
    Ok(())
  }
}


/// similar to ID but using default trait implementation
pub struct DefaultID();
impl ExtRead for DefaultID {
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf: &mut [u8]) -> Result<usize> {
    r.read(buf)
  }
  #[inline]
  fn read_header<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn read_end<R : Read>(&mut self, _ : &mut R) -> Result<()> {
    Ok(())
  }
}


impl ExtWrite for DefaultID {
  #[inline]
  fn write_header<W : Write>(&mut self, _ : &mut W) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn write_end<W : Write>(&mut self, _ : &mut W) -> Result<()> {
    Ok(())
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont: &[u8]) -> Result<usize> {
    w.write(cont)
  }
}


impl<'a, EW : ExtWrite> ExtWrite for &'a mut EW {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    (*self).write_header(w)
  }
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    (*self).write_end(w)
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont: &[u8]) -> Result<usize> {
    (*self).write_into(w,cont)
  }
  #[inline]
  fn write_all_into<W : Write>(&mut self, w : &mut W, cont: &[u8]) -> Result<()> {
    (*self).write_all_into(w,cont)
  }
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {
    (*self).flush_into(w)
  }
}



/// partial extread to reuse default implementation explicitly
pub struct ReadDefImpl<'a,R : ExtRead + 'a>(pub &'a mut R);
impl<'a,RE : ExtRead> ExtRead for ReadDefImpl<'a,RE> {
  #[inline]
  fn read_from<R : Read>(&mut self, r : &mut R, buf: &mut [u8]) -> Result<usize> {
    self.0.read_from(r,buf)
  }
  #[inline]
  fn read_header<R : Read>(&mut self, r : &mut R) -> Result<()> {
    self.0.read_header(r)
  }
  #[inline]
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    self.0.read_end(r)
  }
}


/// partial extread to reuse default implementation explicitly
pub struct WriteDefImpl<'a,W : ExtWrite + 'a>(pub &'a mut W);

impl<'a,WR : ExtWrite> ExtWrite for WriteDefImpl<'a,WR> {
  #[inline]
  fn write_header<W : Write>(&mut self, w : &mut W) -> Result<()> {
    self.0.write_header(w)
  }
  #[inline]
  fn write_end<W : Write>(&mut self, w : &mut W) -> Result<()> {
    self.0.write_end(w)
  }
  #[inline]
  fn write_into<W : Write>(&mut self, w : &mut W, cont: &[u8]) -> Result<usize> {
    self.0.write_into(w,cont)
  }
}

// TODO loop reader struct where on read 0 we read end automatically and read header again
//
