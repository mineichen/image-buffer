# Image buffers

This crate provides image buffers, which can be used to hide the underlying storage.

Image

- FFI compatible
- Ability to support buffers from other libraries like "opencv" without copying (type erasure)
- Copy on write capability, so buffers can be reused if the inner representation is not shared
