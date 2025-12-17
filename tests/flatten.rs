use std::num::NonZeroU32;

use imbuf::Image;

#[test]
fn test_flatten() {
    let two = NonZeroU32::new(2).unwrap();
    let input = vec![[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]];
    let image = Image::<[u8; 3], 1>::new_vec(input.clone(), two, two);
    let buffers = image.buffer();
    assert_eq!(buffers, &input);
    assert_eq!(image.flat_buffer(), (0..12).collect::<Vec<_>>());
}

// #[test]
// fn test_comptime_error() {
//     let x = Image::<u8, 0>::new_vec(
//         vec![0u8],
//         NonZeroU32::new(1).unwrap(),
//         NonZeroU32::new(1).unwrap(),
//     );
// }
