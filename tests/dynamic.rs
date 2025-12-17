use std::num::NonZeroU32;

use imbuf::{DynamicImage, Image, ImageChannel};

const TWO: NonZeroU32 = NonZeroU32::new(2).unwrap();

#[test]
fn create_from_dynamic_with_different_size() {
    let image: Image<f32, 3> =
        Image::new_vec(vec![42f32, 42., 43.], NonZeroU32::MIN, NonZeroU32::MIN);

    let dynamic = DynamicImage::from(image.clone());
    let back: Image<f32, 3> = dynamic.try_into().unwrap();
    assert_eq!(back, image);

    let mut dynamic = DynamicImage::from(image);
    dynamic[0] = ImageChannel::new_vec(vec![42f32, 42.], NonZeroU32::MIN, TWO).into();

    let back = <Image<f32, 3>>::try_from(dynamic.clone()).unwrap_err();
    assert_eq!(back.image, dynamic);
}
