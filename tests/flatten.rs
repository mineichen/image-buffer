use std::num::NonZeroU32;

use imbuf::Image;

#[test]
fn test_flatten() {
    let two = NonZeroU32::new(2).unwrap();
    let input = vec![[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]];
    let image = Image::<[u8; 3], 1>::new_vec(input.clone(), two, two);
    let buffers = image.buffers()[0];
    assert_eq!(buffers, &input);
    assert_eq!(image.flat_buffer(), (0..12).collect::<Vec<_>>());
}

// #[test]
// fn test_miri_cast() {
//     let x: &[u8] = &[0, 1, 2];
//     let y: &[[u8; 3]] = unsafe { std::slice::from_raw_parts(x.as_ptr() as *const [u8; 3], 1) };
//     dbg!(y);
//     assert_eq!(y[0][2], 2);
// }
