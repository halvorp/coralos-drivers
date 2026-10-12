#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdbool.h>
typedef uint8_t u8; typedef uint16_t u16; typedef uint32_t u32; typedef uint64_t u64;
#define U16_MAX 0xffff
#define U32_MAX 0xffffffffu
#define UINT_MAX 0xffffffffu
#define WARN_ON(x) (x)
#define DIV_ROUND_UP(n,d) (((n)+(d)-1)/(d))
#define DIV_ROUND_UP_ULL(n,d) ((u64)(((u64)(n)+(u64)(d)-1)/(u64)(d)))
#define DIV64_U64_ROUND_UP(n,d) DIV_ROUND_UP_ULL(n,d)
#define DIV_ROUND_CLOSEST(n,d) (((n)+(d)/2)/(d))
#define mul_u32_u32(a,b) ((u64)(u32)(a)*(u64)(u32)(b))
#define min(a,b) ((a)<(b)?(a):(b))
#define max(a,b) ((a)>(b)?(a):(b))
#define min_t(t,a,b) ((t)(a)<(t)(b)?(t)(a):(t)(b))
#define max_t(t,a,b) ((t)(a)>(t)(b)?(t)(a):(t)(b))
#define REG_GENMASK(h,l) ((~0u>>(31-(h)))&(~0u<<(l)))
#define REG_FIELD_GET(m,v) ((((u32)(v))&(m))>>__builtin_ctz(m))
#define BIT(n) (1u<<(n))
#define hweight8(x) __builtin_popcount((u8)(x))
#define drm_dbg_kms(...) do{}while(0)
#define drm_WARN_ON(d,x) (x)
#define drm_WARN(...) 0
#define MISSING_CASE(x) do{}while(0)
#define IS_DISPLAY_VER(d,a,b) 0
#define DISPLAY_VER(d) 9
#define EINVAL 22
#define DRM_FORMAT_ARGB8888 1
#define DRM_FORMAT_XRGB8888 2
#define DRM_FORMAT_NV12 3
#define DRM_FORMAT_MOD_LINEAR 0ull
#define I915_FORMAT_MOD_X_TILED 1ull
#define I915_FORMAT_MOD_Y_TILED 2ull
#define I915_FORMAT_MOD_Yf_TILED 3ull
#define DRM_MODE_ROTATE_0 1
#define drm_rotation_90_or_270(r) 0
struct drm_format_info { u32 format; u8 cpp[4]; int num_planes; bool is_yuv; };
static const struct drm_format_info fi_argb={DRM_FORMAT_ARGB8888,{4},1,false};
static const struct drm_format_info fi_xrgb={DRM_FORMAT_XRGB8888,{4},1,false};
static const struct drm_format_info fi_nv12={DRM_FORMAT_NV12,{1,2},2,true};
static bool intel_format_info_is_yuv_semiplanar(const struct drm_format_info*f,u64 m){return f->num_planes==2;}
static bool intel_fb_is_tiled_modifier(u64 m){return m!=0;}
static bool intel_fb_is_ccs_modifier(u64 m){return false;}
enum plane_id { PLANE_1, PLANE_2, PLANE_3, PLANE_4, PLANE_5, PLANE_6, PLANE_7, PLANE_CURSOR, I915_MAX_PLANES };
enum pipe { PIPE_A };
struct skl_ddb_entry { u16 start, end; };
static inline u16 skl_ddb_entry_size(const struct skl_ddb_entry *e){ return e->end - e->start; }
struct skl_wm_level { u16 min_ddb_alloc; u16 blocks; u8 lines; bool enable; bool ignore_lines; bool auto_min_alloc_wm_enable; bool can_sagv; };
struct skl_plane_wm { struct skl_wm_level wm[8]; struct skl_wm_level uv_wm[8]; struct skl_wm_level trans_wm; struct { struct skl_wm_level wm0; struct skl_wm_level trans_wm; } sagv; bool is_planar; };
struct skl_pipe_wm { struct skl_plane_wm planes[I915_MAX_PLANES]; bool use_sagv_wm; };
struct intel_plane { enum plane_id id; bool async_flip; };
struct intel_crtc { enum pipe pipe; u32 plane_ids_mask; struct intel_plane *cursor_p; };
struct dram_info { bool has_16gb_dimms; bool symmetric_memory; };
struct drm_mode_config { int cursor_width; };
struct drm_device_s { struct drm_mode_config mode_config; };
static struct drm_device_s g_drm;
struct intel_display {
  struct { int num_levels; u16 skl_latency[8]; bool ipc_enabled; } wm;
  struct { int block_time_us; } sagv;
  struct { bool kabylake, coffeelake, cometlake, skylake, dg2, broxton, geminilake; } platform;
  struct drm_device_s *drm;
};
static struct intel_display g_display;
static struct dram_info g_dram;
static const struct dram_info *intel_dram_info(struct intel_display *d){return &g_dram;}
#define to_intel_display(x) (&g_display)
static bool skl_watermark_ipc_enabled(struct intel_display *d){ return d->wm.ipc_enabled; }
struct drm_display_mode_s { int crtc_htotal, crtc_hdisplay; };
struct intel_crtc_state {
  struct { bool active; struct drm_display_mode_s pipe_mode; } hw;
  struct { bool async_flip; struct { struct intel_plane *cursor; } *crtc; } uapi;
  u32 pixel_rate;
  struct { struct { struct skl_ddb_entry plane_ddb[I915_MAX_PLANES], plane_ddb_y[I915_MAX_PLANES]; u16 plane_min_ddb[I915_MAX_PLANES], plane_interim_ddb[I915_MAX_PLANES]; struct skl_pipe_wm optimal; } skl; } wm;
  u32 rel_data_rate[I915_MAX_PLANES], rel_data_rate_y[I915_MAX_PLANES];
  u32 nv12_planes;
};
struct intel_dbuf_state { struct skl_ddb_entry ddb[3]; u8 active_pipes; };
struct intel_atomic_state { struct intel_crtc_state cs; struct intel_dbuf_state ds; };
#define intel_atomic_get_new_crtc_state(s,c) (&(s)->cs)
#define intel_atomic_get_new_dbuf_state(s) (&(s)->ds)
#define for_each_plane_id_on_crtc(crtc,id) for((id)=0;(id)<I915_MAX_PLANES;(id)++) if((crtc)->plane_ids_mask & BIT(id))
#define drm_get_format_info(drm,fmt,mod) ((fmt)==DRM_FORMAT_ARGB8888?&fi_argb:&fi_xrgb)

struct ucrtc_s { struct intel_plane *cursor; };
static struct intel_crtc g_crtc;
#define to_intel_crtc(x) (&g_crtc)
#define to_intel_plane(x) (x)
