#include "mn_fns.inc"
#include <stdlib.h>
int main(int argc,char**argv){ int bpp=atoi(argv[1]), lanes=atoi(argv[2]), pclk=atoi(argv[3]), lclk=atoi(argv[4]);
 struct intel_link_m_n m; intel_link_compute_m_n(bpp*16,lanes,pclk,lclk,1000000,&m);
 printf("%u %u %u %u %u\n",m.tu,m.data_m,m.data_n,m.link_m,m.link_n); return 0;}
