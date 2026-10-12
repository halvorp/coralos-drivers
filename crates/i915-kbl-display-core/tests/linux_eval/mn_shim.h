#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>
typedef uint8_t u8; typedef uint16_t u16; typedef uint32_t u32; typedef uint64_t u64;
#define DATA_LINK_M_N_MASK 0xffffff
#define DATA_LINK_N_MAX 0x800000
#define min_t(t,a,b) ((t)(a)<(t)(b)?(t)(a):(t)(b))
#define DIV_ROUND_UP_ULL(n,d) ((u64)(((u64)(n)+(u64)(d)-1)/(u64)(d)))
#define DIV_ROUND_DOWN_ULL(n,d) ((u64)((u64)(n)/(u64)(d)))
#define DIV_ROUND_CLOSEST(n,d) (((n)+(d)/2)/(d))
#define mul_u32_u32(a,b) ((u64)(u32)(a)*(u64)(u32)(b))
#define div_u64(a,b) ((u64)(a)/(u64)(b))
static inline u32 roundup_pow_of_two(u32 n){ u32 r=1; while(r<n) r<<=1; return r; }
struct intel_link_m_n { u32 tu, data_m, data_n, link_m, link_n; };
static inline bool drm_dp_is_uhbr_rate(int link_rate){ return link_rate >= 1000000; }
#define EXPORT_SYMBOL(x)
