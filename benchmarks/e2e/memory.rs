use dssim_core::Dssim;
use rgb::RGBA;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Count;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn added(size: usize) { let n = LIVE.fetch_add(size, Relaxed) + size; PEAK.fetch_max(n, Relaxed); }
// Benchmark-only allocator: forward unchanged pointer/layout pairs to System.
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { let p=unsafe {System.alloc(l)}; if !p.is_null(){added(l.size());} p }
    unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc_zeroed(l)};if !p.is_null(){added(l.size());}p}
    unsafe fn dealloc(&self,p:*mut u8,l:Layout){unsafe{System.dealloc(p,l)};LIVE.fetch_sub(l.size(),Relaxed);}
    unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{let q=unsafe{System.realloc(p,l,n)};if !q.is_null(){if n>=l.size(){added(n-l.size());}else{LIVE.fetch_sub(l.size()-n,Relaxed);}}q}
}
#[global_allocator] static ALLOC: Count = Count;
fn main(){
 let args:Vec<_>=std::env::args().collect();
 let w=args.get(2).map(|s|s.parse().unwrap()).unwrap_or(2049);
 let h=args.get(3).map(|s|s.parse().unwrap()).unwrap_or(1024);
 let rgba=args.get(1).is_none_or(|s|s=="rgba");
 let d=Dssim::new();
 let mode=std::env::var("DSSIM_MODE").unwrap_or_else(|_| "cc".into());
 // Initialize Rayon before recording the input baseline.
 drop(d.create_image_rgba(&vec![RGBA::new(1,2,3,255);64],8,8));
 let (a,b,input_live)=if rgba {
  let src:Vec<_>=(0..w*h).map(|i|{let v=(i as u32).wrapping_mul(2654435761);RGBA::new(v as u8,(v>>8)as u8,(v>>16)as u8,(v>>24)as u8)}).collect();
  let dst:Vec<_>=src.iter().map(|p|RGBA::new(p.r.wrapping_add(3),p.g,p.b,p.a)).collect();
  let n=LIVE.load(Relaxed);PEAK.store(n,Relaxed);

  #[cfg(feature = "options")]
  let (a,b) = (
    d.create_image_rgba_with_options(&src,w,h,dssim_core::ImageOptions::default().cache_for_reuse(mode.as_bytes()[0] == b'c')).unwrap(),
    d.create_image_rgba_with_options(&dst,w,h,dssim_core::ImageOptions::default().cache_for_reuse(mode.as_bytes()[1] == b'c')).unwrap(),
  );
  #[cfg(not(feature = "options"))]
  let (a,b) = { assert_eq!(mode, "cc"); (d.create_image_rgba(&src,w,h).unwrap(), d.create_image_rgba(&dst,w,h).unwrap()) };
  // Record while input buffers remain live, then preserve them through compare.
  println!("input_bytes,{n}");println!("prepared_bytes,{}",LIVE.load(Relaxed)-n);println!("prepare_peak_extra,{}",PEAK.load(Relaxed)-n);
  (a,b,(src,dst,n))
 } else { unreachable!("use rgba mode for the isolated allocation measurement") };
 let before=LIVE.load(Relaxed);PEAK.store(before,Relaxed);
 let (score,maps)=black_box(d.compare(&a,&b));
 let extra=PEAK.load(Relaxed)-before;
 println!("compare_peak_extra,{extra}");println!("score_bits,{:016x}",f64::from(score).to_bits());
 black_box((a,b,maps,input_live));
}
