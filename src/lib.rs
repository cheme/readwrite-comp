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
//! Current issue with this crate
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
#![feature(slice_bytes)] // TODO deprecated from 1.6

#[cfg(test)]
mod test;

#[cfg(test)]
extern crate rand;

use std::io::{
  Write,
  Read,
  Result,
};
use std::ops::Drop;
use std::mem::replace;

/// Write with further common functionnalities.
/// 
/// Compose over another Writer
/// TODO eg crypter
pub trait ExtWrite {

  /// write header if needed
  fn write_header<W : Write>(&mut self, &mut W) -> Result<()>;

  /// write buffer.
  fn write_into<W : Write>(&mut self, &mut W, &[u8]) -> Result<usize>;

  /// Could add end content (padding...) only if read can manage it
  /// does not flush recursivly
  #[inline]
  fn flush_into<W : Write>(&mut self, w : &mut W) -> Result<()> {Ok(())}

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

  /// read end bytes (and possibly update internal state).
  /// To use in a pure read write context, this is call on CompR Drop and should generally not need to be called manually.
  /// When the outer element of composition is removed drop finalize its action.
  fn read_end<R : Read>(&mut self, &mut R) -> Result<()>;

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
/// TODO compWOwn -> CompW and CompW to CompWRef
pub struct CompWOwn<'a, W : 'a + Write, EW : ExtWrite>(pub &'a mut W, pub EW, pub CompWState);

/// inner struct for implemention just to apply method of sw in write
struct CompExtWInner<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite>(&'a mut W, &'b mut EW);

impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> Write for CompExtWInner<'a, 'b, W, EW> {
  #[inline]
  fn write(&mut self, cont: &[u8]) -> Result<usize> {
    self.1.write_into(self.0, cont)
  }
  #[inline]
  fn flush(&mut self) -> Result<()> {
    self.1.flush_into(self.0)
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


pub struct CompExtR<EW1 : ExtRead, EW2 : ExtRead>(EW1, EW2);
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
  fn read_end<R : Read>(&mut self, r : &mut R) -> Result<()> {
    try!(self.0.read_end(&mut CompExtRInner(r, &mut self.1)));
    self.1.read_end(r)
  }

}

struct CompExtRInner<'a, 'b, R : 'a + Read, ER : 'b + ExtRead>(&'a mut R, &'b mut ER);
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Read for CompExtRInner<'a,'b,R,ER> {
  #[inline]
  fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
    self.1.read_from(self.0, buf)
  }
}
 

/// drop finalize but without catching possible issue TODO error mgmt?? include logger ? Or upt to
/// ExtWrite write_end implementation to be safe (return allways ok and synch over shared variable
/// like arcmut or jus rc cell and have something else managing this error
/// Drop is only for reference CompW, for Own compw drop does not exists and should no be used as 
/// a Write.
impl<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> Drop for CompW<'a,'b,W,EW> {
  fn drop(&mut self) {
    if let CompWState::HeadWritten = self.2 {
      self.write_end();
      self.flush();
    }
  }
}


/// drop finalize but without catching possible issue TODO error mgmt?? include logger ?
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Drop for CompR<'a,'b,R,ER> {
  fn drop(&mut self) {
    if let CompRState::HeadRead = self.2 {
      self.read_end();
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
}

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
/// incomplet write : no write end support.
impl<'a, W : 'a + Write, EW : ExtWrite> Write for CompWOwn<'a,W,EW> {
  write_impl_comp!();
}



/// TODO find a way to incorporate read end in Read of compR (for the moment you need to at least
/// use set_end (CompR fn). On Read_0 : no. TODO study other read functions.
impl<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> Read for CompR<'a,'b,R,ER> {
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
pub type MultiW<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> = CompW<'a,'b,W,MultiWExt<'b,EW>>;
pub type MultiWOwn<'a, W : 'a + Write, EW : ExtWrite> = CompWOwn<'a,W,MultiWExtOwn<EW>>;
pub struct MultiWExt<'a, EW : 'a + ExtWrite>(&'a mut[EW], Vec<CompWState>);
pub struct MultiWExtOwn<EW : ExtWrite>(Vec<EW>, Vec<CompWState>);

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
pub type MultiR<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> = CompR<'a,'b,R,MultiRExt<'b,ER>>;

pub struct MultiRExt<'a, ER : 'a + ExtRead>(&'a mut[ER], Vec<CompRState>);

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
 
        println!("in write end of {:?}", self.1.len());
        if self.1.len() > 1 {
        if let Some((f,last)) = self.1.split_first_mut()  {
          let mut el = MCompW(self.0, last, &mut self.2[1..]);
          println!("next write end of {:?}", el.1.len());
          try!(f.write_end(&mut el));
          try!(el.write_end());
        }
        } else {
          // last
          try!((self.1).get_mut(0).unwrap().write_end(&mut self.0));
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
          try!((self.1).get_mut(0).unwrap().read_end(&mut self.0));
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
  (w : &'a mut W, ew : &'b mut MultiWExt<'b,EW>) -> MultiW<'a,'b,W,EW> {
    CompW::new(w, ew)
}
#[inline]
pub fn new_multiwown<'a, W : 'a + Write, EW : ExtWrite> 
  (w : &'a mut W, ew : Vec<EW>) -> MultiWOwn<'a,W,EW> {
    CompWOwn::new(w, MultiWExtOwn::new(ew))
}

#[inline]
pub fn new_multir<'a, 'b, R : 'a + Read, ER : 'b + ExtRead> 
  (r : &'a mut R, er : &'b mut MultiRExt<'b,ER>) -> MultiR<'a,'b,R,ER> {
    CompR::new(r, er)
}

/*
// TODO try a box new multiw (compw need to use as_ref))
pub fn new_multiw<'a, 'b, W : 'a + Write, EW : 'b + ExtWrite> 
  (w : &'a mut W, ew : &'b mut [EW]) -> MultiW<'a,'b,W,EW> {
    CompW::new(w, Box::new(MultiWExt::new(ew)))
}
*/
impl<'a, EW : 'a + ExtWrite> MultiWExt<'a,EW> {
  #[inline]
  fn inner<'c,'b, W : Write>(&'c mut self, w : &'b mut W) -> MCompW<'b,'c,W,EW> {
    MCompW(w,self.0,&mut self.1[..])
  }
  #[inline]
  pub fn new(ew : &'a mut [EW]) -> Self {
    let state = MultiWExtOwn::init_state(ew);
    MultiWExt(ew,state)
  }

}
impl<EW : ExtWrite> MultiWExtOwn<EW> {
  #[inline]
  fn inner<'c,'b, W : Write>(&'c mut self, w : &'b mut W) -> MCompW<'b,'c,W,EW> {
    MCompW(w,&mut self.0[..],&mut self.1[..])
  }
  #[inline]
  pub fn new(ew : Vec<EW>) -> Self {
    let state = Self::init_state(&ew[..]);
    MultiWExtOwn(ew,state)
  }

  #[inline]
  pub fn init_state(ew : &[EW]) -> Vec<CompWState> {
    vec![CompWState::Initial; ew.len()]
  }
}


impl<'a, ER : 'a + ExtRead> MultiRExt<'a,ER> {
  #[inline]
  fn inner<'c,'b,R : Read>(&'c mut self, r : &'b mut R) -> MCompR<'b,'c,R,ER> {
    MCompR(r,self.0,&mut self.1[..])
  }
  #[inline]
  pub fn new(ew : &'a mut [ER]) -> Self {
    let state = Self::init_state(ew);
    MultiRExt(ew,state)
  }
  #[inline]
  pub fn init_state(ew : &mut [ER]) -> Vec<CompRState> {
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

impl<'a, EW : 'a + ExtWrite> ExtWrite for MultiWExt<'a,EW> {
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
impl<EW : ExtWrite> ExtWrite for MultiWExtOwn<EW> {
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


impl<'a, EW : 'a + ExtRead> ExtRead for MultiRExt<'a,EW> {
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


