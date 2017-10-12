extern crate readwrite_comp;
extern crate rand;
use readwrite_comp::{
  ExtWrite,
  ExtRead,
};

use std::io::{
  Write,
  Read,
  Cursor,
  Result,
  //Error,
  //ErrorKind,
};

use rand::Rng;
use rand::os::OsRng;


pub fn test_bytes_wr<BW : ExtWrite, BR : ExtRead> 
  (inp_length : usize, buf_length : usize, bw : &mut BW, br : &mut BR) -> Result<()> {
  let mut rng = OsRng::new().unwrap();
  let mut outputb = Cursor::new(Vec::new());
  let mut reference = Cursor::new(Vec::new());
  let output = &mut outputb;
  let mut bufb = vec![0;buf_length];
  let mut has_started = false;
  let buf = &mut bufb;
  // knwoledge of size to write
  let mut i = 0;
  while inp_length > i {
    rng.fill_bytes(buf);
    if !has_started {
      try!(bw.write_header(output));
      has_started = true;
    };
    let ww = if inp_length - i < buf.len() {
      try!(reference.write(&buf[..inp_length - i]));
      try!(bw.write_into(output,&buf[..inp_length - i])) 
    } else {
      try!(reference.write(buf));
      try!(bw.write_into(output,buf))
    };
    assert!(ww != 0);
    i += ww;
  }
  try!(bw.write_end(output));
  // write next content
  rng.fill_bytes(buf);
  let endcontent = buf[0];
  println!("EndContent{}",endcontent);
  try!(output.write(&buf[..1]));
  output.flush().unwrap();

  // no knowledge of size to read
  output.set_position(0);
  let input = output;
  has_started = false;
  let mut rr = 1;
  i = 0;
  while rr != 0 {
    if !has_started {
      try!(br.read_header(input));
      has_started = true;
    }
    rr = try!(br.read_from(input,buf));
    // it could go over reference as no knowledge (if padding)
    // inp length is here only to content assertion
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
  try!(br.read_end(input));
  assert!(i >= inp_length);
  let ni = try!(input.read(buf));
  assert!(ni == 1);
  assert!(endcontent == buf[0]);
  Ok(())

}

