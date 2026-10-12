#include "wm_shim.h"
#include "intel_fixed.h"
#include "wm_fns.inc"

static void dump_level(const char*tag,int lv,const struct skl_wm_level*w){
  printf("%s %d en=%d blocks=%u lines=%u min=%u\n",tag,lv,w->enable,w->blocks,w->lines,w->min_ddb_alloc);
}
/* args: clock htotal width fmt(0 xrgb,1 nv12) tiling(0 lin,1 X,2 Y) ipc dram16 lat_first lat_second */
int main(int argc,char**argv){
  u32 clock=atoi(argv[1]), htotal=atoi(argv[2]), width=atoi(argv[3]), height=atoi(argv[4]);
  int fmt=atoi(argv[5]); int tiling=atoi(argv[6]); int ipc=atoi(argv[7]); int d16=atoi(argv[8]);
  u32 lf=strtoul(argv[9],0,0), ls=strtoul(argv[10],0,0);
  struct intel_display*d=&g_display; d->drm=&g_drm; g_drm.mode_config.cursor_width=256;
  d->platform.kabylake=true; d->wm.num_levels=8; d->wm.ipc_enabled=ipc; g_dram.has_16gb_dimms=d16; g_dram.symmetric_memory=ipc;
  u16*wm=d->wm.skl_latency;
  wm[0]=REG_FIELD_GET(REG_GENMASK(7,0),lf); wm[1]=REG_FIELD_GET(REG_GENMASK(15,8),lf); wm[2]=REG_FIELD_GET(REG_GENMASK(23,16),lf); wm[3]=REG_FIELD_GET(REG_GENMASK(31,24),lf);
  wm[4]=REG_FIELD_GET(REG_GENMASK(7,0),ls); wm[5]=REG_FIELD_GET(REG_GENMASK(15,8),ls); wm[6]=REG_FIELD_GET(REG_GENMASK(23,16),ls); wm[7]=REG_FIELD_GET(REG_GENMASK(31,24),ls);
  adjust_wm_latency(d);
  printf("LAT"); for(int i=0;i<8;i++) printf(" %u",wm[i]); printf("\n");
  static struct intel_atomic_state st; memset(&st,0,sizeof st);
  struct intel_crtc_state*cs=&st.cs; g_crtc.pipe=PIPE_A; g_crtc.plane_ids_mask=BIT(PLANE_1)|BIT(PLANE_CURSOR);
  static struct ucrtc_s ucrtc; static struct intel_plane cur={PLANE_CURSOR,false}; ucrtc.cursor=&cur; cs->uapi.crtc=(void*)&ucrtc;
  cs->hw.active=true; cs->hw.pipe_mode.crtc_htotal=htotal; cs->pixel_rate=clock;
  static struct intel_plane p1={PLANE_1,false};
  u64 mod = tiling==0?DRM_FORMAT_MOD_LINEAR:(tiling==1?I915_FORMAT_MOD_X_TILED:I915_FORMAT_MOD_Y_TILED);
  struct skl_plane_wm*pw=&cs->wm.skl.optimal.planes[PLANE_1];
  struct skl_wm_params wp;
  const struct drm_format_info*info = fmt==1?&fi_nv12:&fi_xrgb;
  int r=skl_compute_wm_params(cs,width,info,mod,DRM_MODE_ROTATE_0,clock,&wp,0,0); if(r){printf("ERR params %d\n",r);return 1;}
  printf("WP bpl=%u pbpl=%u ytm=%u ymin=%u linetime=%u cpp=%u\n",wp.plane_bytes_per_line,wp.plane_blocks_per_line.val,wp.y_tile_minimum.val,wp.y_min_scanlines,wp.linetime_us,wp.cpp);
  skl_compute_wm_levels(cs,&p1,&wp,pw->wm);
  if(fmt==1){ struct skl_wm_params wu; r=skl_compute_wm_params(cs,width,info,mod,DRM_MODE_ROTATE_0,clock,&wu,1,0); if(r){printf("ERR uv\n");return 1;}
    skl_compute_wm_levels(cs,&p1,&wu,pw->uv_wm); pw->is_planar=true; cs->nv12_planes=BIT(PLANE_1);
    cs->rel_data_rate_y[PLANE_1]=width*height*1; cs->rel_data_rate[PLANE_1]=(width/2)*(height/2)*2; }
  else cs->rel_data_rate[PLANE_1]=width*height*4;
  st.ds.ddb[PIPE_A].start=0; st.ds.ddb[PIPE_A].end=892; st.ds.active_pipes=1;
  r=skl_crtc_allocate_plane_ddb(&st,&g_crtc); if(r){printf("ERR ddb %d\n",r);return 0;}
  for(int l=0;l<8;l++) dump_level("WM",l,&pw->wm[l]);
  if(fmt==1) for(int l=0;l<8;l++) dump_level("UV",l,&pw->uv_wm[l]);
  dump_level("TR",0,&pw->trans_wm);
  printf("DDB p1 %u %u y %u %u cur %u %u\n",cs->wm.skl.plane_ddb[PLANE_1].start,cs->wm.skl.plane_ddb[PLANE_1].end,cs->wm.skl.plane_ddb_y[PLANE_1].start,cs->wm.skl.plane_ddb_y[PLANE_1].end,cs->wm.skl.plane_ddb[PLANE_CURSOR].start,cs->wm.skl.plane_ddb[PLANE_CURSOR].end);
  return 0; }
