#![allow(unused, dead_code)]

#[macro_use]
extern crate structopt;
extern crate fuse;
extern crate nbd;
extern crate readwriteseekfs;
extern crate bufstream;
extern crate rand;

use std::process::Command;
use rand::{thread_rng, Rng};
use std::time::Duration;
use std::thread::sleep;

use fuse::Filesystem;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use structopt::StructOpt;

use std::fs::File;
use std::io::{Error, ErrorKind, Result};

use std::net::TcpStream;
use std::os::unix::net::UnixStream;

use nbd::client::{handshake, NbdClient};
use readwriteseekfs::{ReadSeekFs,ReadWriteSeekFs};

#[derive(StructOpt, Debug)]
#[structopt(
    after_help = "
Example:
    fuseqemu image.qcow image.raw -f qcow2
    
    fuseqemu -r image.qcow image.raw -f qcow2 -- -o allow_empty,ro,fsname=qwerty,auto_unmount
",
)]
struct Opt {
    /// Path to image
    image: String,
    /// Regular file to use as mountpoint
    #[structopt(parse(from_os_str))]
    file: PathBuf,
    /// Named export to use.
    #[structopt(short = "-x", long = "export-name", default_value = "")]
    export: String,

    /// Image format (see qemu-nbd; e.g., raw, qcow2, ...)
    #[structopt(short = "f", long = "format", default_value = "")]
    format: String,

    /// Additional option passed to qemu-nbd
    #[structopt(short = "o", long = "qemu-opt")]
    qemuopts: Vec<String>,

    /// qemu-nbd cache mode
    #[structopt(long = "cache", default_value = "unsafe")]
    cache: String,

    /// Mount read-only
    #[structopt(short = "r", long = "read-only")]
    ro: bool,

    /// Modify reported size of image
    #[structopt(short = "s", long = "size")]
    size: Option<u64>,

    /// The rest of FUSE options.
    #[structopt(parse(from_os_str))]
    opts: Vec<OsString>,
}

fn temp_path() -> PathBuf {
    let tmp: String = thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(50)
        .collect();
    std::env::temp_dir().join(tmp)
}

fn run() -> Result<()> {
    let mut qemunbd_process;
let res = {
    let mut cmd = Opt::from_args();

    let socket_path = temp_path();
    let mut qemunbd = Command::new("qemu-nbd");
    qemunbd.arg("-k").arg(&socket_path);
    if cmd.export != "" {
        qemunbd.arg("-x").arg(&cmd.export);
    }
    if cmd.format != "" {
        qemunbd.arg("-f").arg(cmd.format);
    }
    qemunbd.arg("--cache").arg(&cmd.cache);
    for opt in cmd.qemuopts {
        qemunbd.arg(opt);
    }
    qemunbd.arg(cmd.image);

    qemunbd_process = qemunbd.spawn()?;

    while socket_path.metadata().is_err() {
        sleep(Duration::from_millis(100));
    }

    match cmd.file.metadata() {
        Ok(ref m) if m.is_dir() => eprintln!("Warning: {:?} is a directory, not a file", cmd.file),
        Ok(ref m) if m.is_file() => (),
        Ok(_) => eprintln!("Warning: can't determine type of {:?}", cmd.file),
        Err(ref e) if e.kind() == ErrorKind::NotFound => {
            drop(File::create(cmd.file.clone()));
        }
        Err(e) => Err(e)?,
    }

    let mut tcp = UnixStream::connect(&socket_path)?;
    let mut tcp = bufstream::BufStream::new(tcp);
    let mut export = handshake(&mut tcp, cmd.export.as_bytes())?;
    if let Some(size) = cmd.size {
        export.size = size;
    }
    let mut client = NbdClient::new(&mut tcp, &export);

    let default_fuse_opts = vec!["-o", "auto_unmount"];
    let mut opts: Vec<&OsStr> = cmd.opts.iter().map(AsRef::as_ref).collect();

    if opts.len() == 0 {
        opts = default_fuse_opts.iter().map(OsStr::new).collect();
    }
    
    if cmd.ro {
        let fs = readwriteseekfs::ReadSeekFs::new(client, 1024)?;
        fuse::mount(fs, &cmd.file.as_path(), opts.as_slice())
    } else {
        let fs = readwriteseekfs::ReadWriteSeekFs::new(client, 1024)?;
        fuse::mount(fs, &cmd.file.as_path(), opts.as_slice())
    }
};
    qemunbd_process.wait();
    res
}

fn main() {
    let r = run();

    if let Err(e) = r {
        eprintln!("fuseqemu: {}", e);
        ::std::process::exit(1);
    }
}
