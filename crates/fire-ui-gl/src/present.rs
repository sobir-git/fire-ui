use glow::HasContext;
/// Copy the retained image without running a full-window texture shader.
/// Older GL contexts retain the canvas-based presentation path.
pub(crate) struct Presenter {
    gl: glow::Context,
    framebuffer: glow::Framebuffer,
}
impl Presenter {
    /// The loader must belong to the current context, which outlives this object.
    pub(crate) unsafe fn new(
        loader: impl FnMut(&std::ffi::CStr) -> *const std::ffi::c_void,
    ) -> Option<Self> {
        let gl = unsafe { glow::Context::from_loader_function_cstr(loader) };
        if gl.version().major < 3 {
            return None;
        }
        let framebuffer = unsafe { gl.create_framebuffer().ok()? };
        Some(Self { gl, framebuffer })
    }
    pub(crate) fn copy(
        &self,
        texture: glow::Texture,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // SAFETY: runs on the owning UI thread after canvas.flush(), using its live texture.
        unsafe {
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.framebuffer));
            self.gl.framebuffer_texture_2d(
                glow::READ_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            if self.gl.check_framebuffer_status(glow::READ_FRAMEBUFFER)
                != glow::FRAMEBUFFER_COMPLETE
            {
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                return Err("Retained image is not a complete framebuffer".into());
            }
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.blit_framebuffer(
                0,
                0,
                width as i32,
                height as i32,
                0,
                0,
                width as i32,
                height as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Ok(())
    }
}
impl Drop for Presenter {
    fn drop(&mut self) {
        // SAFETY: the host drops GPU resources before releasing its current context.
        unsafe {
            self.gl.delete_framebuffer(self.framebuffer);
        }
    }
}
