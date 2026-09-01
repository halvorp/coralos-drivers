// SPDX-License-Identifier: GPL-2.0-only
//! Cherryview pin descriptor corpus, ported from Linux
//! `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on work by Ning Li and Alan Cox.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub number: u16,
    pub name: &'static str,
}

pub const SOUTHWEST_PINS: [Pin; 56] = [
    Pin {
        number: 0,
        name: "FST_SPI_D2",
    }, // pinctrl-cherryview.c:104
    Pin {
        number: 1,
        name: "FST_SPI_D0",
    }, // pinctrl-cherryview.c:105
    Pin {
        number: 2,
        name: "FST_SPI_CLK",
    }, // pinctrl-cherryview.c:106
    Pin {
        number: 3,
        name: "FST_SPI_D3",
    }, // pinctrl-cherryview.c:107
    Pin {
        number: 4,
        name: "FST_SPI_CS1_B",
    }, // pinctrl-cherryview.c:108
    Pin {
        number: 5,
        name: "FST_SPI_D1",
    }, // pinctrl-cherryview.c:109
    Pin {
        number: 6,
        name: "FST_SPI_CS0_B",
    }, // pinctrl-cherryview.c:110
    Pin {
        number: 7,
        name: "FST_SPI_CS2_B",
    }, // pinctrl-cherryview.c:111
    Pin {
        number: 15,
        name: "UART1_RTS_B",
    }, // pinctrl-cherryview.c:113
    Pin {
        number: 16,
        name: "UART1_RXD",
    }, // pinctrl-cherryview.c:114
    Pin {
        number: 17,
        name: "UART2_RXD",
    }, // pinctrl-cherryview.c:115
    Pin {
        number: 18,
        name: "UART1_CTS_B",
    }, // pinctrl-cherryview.c:116
    Pin {
        number: 19,
        name: "UART2_RTS_B",
    }, // pinctrl-cherryview.c:117
    Pin {
        number: 20,
        name: "UART1_TXD",
    }, // pinctrl-cherryview.c:118
    Pin {
        number: 21,
        name: "UART2_TXD",
    }, // pinctrl-cherryview.c:119
    Pin {
        number: 22,
        name: "UART2_CTS_B",
    }, // pinctrl-cherryview.c:120
    Pin {
        number: 30,
        name: "MF_HDA_CLK",
    }, // pinctrl-cherryview.c:122
    Pin {
        number: 31,
        name: "MF_HDA_RSTB",
    }, // pinctrl-cherryview.c:123
    Pin {
        number: 32,
        name: "MF_HDA_SDIO",
    }, // pinctrl-cherryview.c:124
    Pin {
        number: 33,
        name: "MF_HDA_SDO",
    }, // pinctrl-cherryview.c:125
    Pin {
        number: 34,
        name: "MF_HDA_DOCKRSTB",
    }, // pinctrl-cherryview.c:126
    Pin {
        number: 35,
        name: "MF_HDA_SYNC",
    }, // pinctrl-cherryview.c:127
    Pin {
        number: 36,
        name: "MF_HDA_SDI1",
    }, // pinctrl-cherryview.c:128
    Pin {
        number: 37,
        name: "MF_HDA_DOCKENB",
    }, // pinctrl-cherryview.c:129
    Pin {
        number: 45,
        name: "I2C5_SDA",
    }, // pinctrl-cherryview.c:131
    Pin {
        number: 46,
        name: "I2C4_SDA",
    }, // pinctrl-cherryview.c:132
    Pin {
        number: 47,
        name: "I2C6_SDA",
    }, // pinctrl-cherryview.c:133
    Pin {
        number: 48,
        name: "I2C5_SCL",
    }, // pinctrl-cherryview.c:134
    Pin {
        number: 49,
        name: "I2C_NFC_SDA",
    }, // pinctrl-cherryview.c:135
    Pin {
        number: 50,
        name: "I2C4_SCL",
    }, // pinctrl-cherryview.c:136
    Pin {
        number: 51,
        name: "I2C6_SCL",
    }, // pinctrl-cherryview.c:137
    Pin {
        number: 52,
        name: "I2C_NFC_SCL",
    }, // pinctrl-cherryview.c:138
    Pin {
        number: 60,
        name: "I2C1_SDA",
    }, // pinctrl-cherryview.c:140
    Pin {
        number: 61,
        name: "I2C0_SDA",
    }, // pinctrl-cherryview.c:141
    Pin {
        number: 62,
        name: "I2C2_SDA",
    }, // pinctrl-cherryview.c:142
    Pin {
        number: 63,
        name: "I2C1_SCL",
    }, // pinctrl-cherryview.c:143
    Pin {
        number: 64,
        name: "I2C3_SDA",
    }, // pinctrl-cherryview.c:144
    Pin {
        number: 65,
        name: "I2C0_SCL",
    }, // pinctrl-cherryview.c:145
    Pin {
        number: 66,
        name: "I2C2_SCL",
    }, // pinctrl-cherryview.c:146
    Pin {
        number: 67,
        name: "I2C3_SCL",
    }, // pinctrl-cherryview.c:147
    Pin {
        number: 75,
        name: "SATA_GP0",
    }, // pinctrl-cherryview.c:149
    Pin {
        number: 76,
        name: "SATA_GP1",
    }, // pinctrl-cherryview.c:150
    Pin {
        number: 77,
        name: "SATA_LEDN",
    }, // pinctrl-cherryview.c:151
    Pin {
        number: 78,
        name: "SATA_GP2",
    }, // pinctrl-cherryview.c:152
    Pin {
        number: 79,
        name: "MF_SMB_ALERTB",
    }, // pinctrl-cherryview.c:153
    Pin {
        number: 80,
        name: "SATA_GP3",
    }, // pinctrl-cherryview.c:154
    Pin {
        number: 81,
        name: "MF_SMB_CLK",
    }, // pinctrl-cherryview.c:155
    Pin {
        number: 82,
        name: "MF_SMB_DATA",
    }, // pinctrl-cherryview.c:156
    Pin {
        number: 90,
        name: "PCIE_CLKREQ0B",
    }, // pinctrl-cherryview.c:158
    Pin {
        number: 91,
        name: "PCIE_CLKREQ1B",
    }, // pinctrl-cherryview.c:159
    Pin {
        number: 92,
        name: "GP_SSP_2_CLK",
    }, // pinctrl-cherryview.c:160
    Pin {
        number: 93,
        name: "PCIE_CLKREQ2B",
    }, // pinctrl-cherryview.c:161
    Pin {
        number: 94,
        name: "GP_SSP_2_RXD",
    }, // pinctrl-cherryview.c:162
    Pin {
        number: 95,
        name: "PCIE_CLKREQ3B",
    }, // pinctrl-cherryview.c:163
    Pin {
        number: 96,
        name: "GP_SSP_2_FS",
    }, // pinctrl-cherryview.c:164
    Pin {
        number: 97,
        name: "GP_SSP_2_TXD",
    }, // pinctrl-cherryview.c:165
];

pub const NORTH_PINS: [Pin; 59] = [
    Pin {
        number: 0,
        name: "GPIO_DFX_0",
    }, // pinctrl-cherryview.c:285
    Pin {
        number: 1,
        name: "GPIO_DFX_3",
    }, // pinctrl-cherryview.c:286
    Pin {
        number: 2,
        name: "GPIO_DFX_7",
    }, // pinctrl-cherryview.c:287
    Pin {
        number: 3,
        name: "GPIO_DFX_1",
    }, // pinctrl-cherryview.c:288
    Pin {
        number: 4,
        name: "GPIO_DFX_5",
    }, // pinctrl-cherryview.c:289
    Pin {
        number: 5,
        name: "GPIO_DFX_4",
    }, // pinctrl-cherryview.c:290
    Pin {
        number: 6,
        name: "GPIO_DFX_8",
    }, // pinctrl-cherryview.c:291
    Pin {
        number: 7,
        name: "GPIO_DFX_2",
    }, // pinctrl-cherryview.c:292
    Pin {
        number: 8,
        name: "GPIO_DFX_6",
    }, // pinctrl-cherryview.c:293
    Pin {
        number: 15,
        name: "GPIO_SUS0",
    }, // pinctrl-cherryview.c:295
    Pin {
        number: 16,
        name: "SEC_GPIO_SUS10",
    }, // pinctrl-cherryview.c:296
    Pin {
        number: 17,
        name: "GPIO_SUS3",
    }, // pinctrl-cherryview.c:297
    Pin {
        number: 18,
        name: "GPIO_SUS7",
    }, // pinctrl-cherryview.c:298
    Pin {
        number: 19,
        name: "GPIO_SUS1",
    }, // pinctrl-cherryview.c:299
    Pin {
        number: 20,
        name: "GPIO_SUS5",
    }, // pinctrl-cherryview.c:300
    Pin {
        number: 21,
        name: "SEC_GPIO_SUS11",
    }, // pinctrl-cherryview.c:301
    Pin {
        number: 22,
        name: "GPIO_SUS4",
    }, // pinctrl-cherryview.c:302
    Pin {
        number: 23,
        name: "SEC_GPIO_SUS8",
    }, // pinctrl-cherryview.c:303
    Pin {
        number: 24,
        name: "GPIO_SUS2",
    }, // pinctrl-cherryview.c:304
    Pin {
        number: 25,
        name: "GPIO_SUS6",
    }, // pinctrl-cherryview.c:305
    Pin {
        number: 26,
        name: "CX_PREQ_B",
    }, // pinctrl-cherryview.c:306
    Pin {
        number: 27,
        name: "SEC_GPIO_SUS9",
    }, // pinctrl-cherryview.c:307
    Pin {
        number: 30,
        name: "TRST_B",
    }, // pinctrl-cherryview.c:309
    Pin {
        number: 31,
        name: "TCK",
    }, // pinctrl-cherryview.c:310
    Pin {
        number: 32,
        name: "PROCHOT_B",
    }, // pinctrl-cherryview.c:311
    Pin {
        number: 33,
        name: "SVIDO_DATA",
    }, // pinctrl-cherryview.c:312
    Pin {
        number: 34,
        name: "TMS",
    }, // pinctrl-cherryview.c:313
    Pin {
        number: 35,
        name: "CX_PRDY_B_2",
    }, // pinctrl-cherryview.c:314
    Pin {
        number: 36,
        name: "TDO_2",
    }, // pinctrl-cherryview.c:315
    Pin {
        number: 37,
        name: "CX_PRDY_B",
    }, // pinctrl-cherryview.c:316
    Pin {
        number: 38,
        name: "SVIDO_ALERT_B",
    }, // pinctrl-cherryview.c:317
    Pin {
        number: 39,
        name: "TDO",
    }, // pinctrl-cherryview.c:318
    Pin {
        number: 40,
        name: "SVIDO_CLK",
    }, // pinctrl-cherryview.c:319
    Pin {
        number: 41,
        name: "TDI",
    }, // pinctrl-cherryview.c:320
    Pin {
        number: 45,
        name: "GP_CAMERASB_05",
    }, // pinctrl-cherryview.c:322
    Pin {
        number: 46,
        name: "GP_CAMERASB_02",
    }, // pinctrl-cherryview.c:323
    Pin {
        number: 47,
        name: "GP_CAMERASB_08",
    }, // pinctrl-cherryview.c:324
    Pin {
        number: 48,
        name: "GP_CAMERASB_00",
    }, // pinctrl-cherryview.c:325
    Pin {
        number: 49,
        name: "GP_CAMERASB_06",
    }, // pinctrl-cherryview.c:326
    Pin {
        number: 50,
        name: "GP_CAMERASB_10",
    }, // pinctrl-cherryview.c:327
    Pin {
        number: 51,
        name: "GP_CAMERASB_03",
    }, // pinctrl-cherryview.c:328
    Pin {
        number: 52,
        name: "GP_CAMERASB_09",
    }, // pinctrl-cherryview.c:329
    Pin {
        number: 53,
        name: "GP_CAMERASB_01",
    }, // pinctrl-cherryview.c:330
    Pin {
        number: 54,
        name: "GP_CAMERASB_07",
    }, // pinctrl-cherryview.c:331
    Pin {
        number: 55,
        name: "GP_CAMERASB_11",
    }, // pinctrl-cherryview.c:332
    Pin {
        number: 56,
        name: "GP_CAMERASB_04",
    }, // pinctrl-cherryview.c:333
    Pin {
        number: 60,
        name: "PANEL0_BKLTEN",
    }, // pinctrl-cherryview.c:335
    Pin {
        number: 61,
        name: "HV_DDI0_HPD",
    }, // pinctrl-cherryview.c:336
    Pin {
        number: 62,
        name: "HV_DDI2_DDC_SDA",
    }, // pinctrl-cherryview.c:337
    Pin {
        number: 63,
        name: "PANEL1_BKLTCTL",
    }, // pinctrl-cherryview.c:338
    Pin {
        number: 64,
        name: "HV_DDI1_HPD",
    }, // pinctrl-cherryview.c:339
    Pin {
        number: 65,
        name: "PANEL0_BKLTCTL",
    }, // pinctrl-cherryview.c:340
    Pin {
        number: 66,
        name: "HV_DDI0_DDC_SDA",
    }, // pinctrl-cherryview.c:341
    Pin {
        number: 67,
        name: "HV_DDI2_DDC_SCL",
    }, // pinctrl-cherryview.c:342
    Pin {
        number: 68,
        name: "HV_DDI2_HPD",
    }, // pinctrl-cherryview.c:343
    Pin {
        number: 69,
        name: "PANEL1_VDDEN",
    }, // pinctrl-cherryview.c:344
    Pin {
        number: 70,
        name: "PANEL1_BKLTEN",
    }, // pinctrl-cherryview.c:345
    Pin {
        number: 71,
        name: "HV_DDI0_DDC_SCL",
    }, // pinctrl-cherryview.c:346
    Pin {
        number: 72,
        name: "PANEL0_VDDEN",
    }, // pinctrl-cherryview.c:347
];

pub const EAST_PINS: [Pin; 24] = [
    Pin {
        number: 0,
        name: "PMU_SLP_S3_B",
    }, // pinctrl-cherryview.c:375
    Pin {
        number: 1,
        name: "PMU_BATLOW_B",
    }, // pinctrl-cherryview.c:376
    Pin {
        number: 2,
        name: "SUS_STAT_B",
    }, // pinctrl-cherryview.c:377
    Pin {
        number: 3,
        name: "PMU_SLP_S0IX_B",
    }, // pinctrl-cherryview.c:378
    Pin {
        number: 4,
        name: "PMU_AC_PRESENT",
    }, // pinctrl-cherryview.c:379
    Pin {
        number: 5,
        name: "PMU_PLTRST_B",
    }, // pinctrl-cherryview.c:380
    Pin {
        number: 6,
        name: "PMU_SUSCLK",
    }, // pinctrl-cherryview.c:381
    Pin {
        number: 7,
        name: "PMU_SLP_LAN_B",
    }, // pinctrl-cherryview.c:382
    Pin {
        number: 8,
        name: "PMU_PWRBTN_B",
    }, // pinctrl-cherryview.c:383
    Pin {
        number: 9,
        name: "PMU_SLP_S4_B",
    }, // pinctrl-cherryview.c:384
    Pin {
        number: 10,
        name: "PMU_WAKE_B",
    }, // pinctrl-cherryview.c:385
    Pin {
        number: 11,
        name: "PMU_WAKE_LAN_B",
    }, // pinctrl-cherryview.c:386
    Pin {
        number: 15,
        name: "MF_ISH_GPIO_3",
    }, // pinctrl-cherryview.c:388
    Pin {
        number: 16,
        name: "MF_ISH_GPIO_7",
    }, // pinctrl-cherryview.c:389
    Pin {
        number: 17,
        name: "MF_ISH_I2C1_SCL",
    }, // pinctrl-cherryview.c:390
    Pin {
        number: 18,
        name: "MF_ISH_GPIO_1",
    }, // pinctrl-cherryview.c:391
    Pin {
        number: 19,
        name: "MF_ISH_GPIO_5",
    }, // pinctrl-cherryview.c:392
    Pin {
        number: 20,
        name: "MF_ISH_GPIO_9",
    }, // pinctrl-cherryview.c:393
    Pin {
        number: 21,
        name: "MF_ISH_GPIO_0",
    }, // pinctrl-cherryview.c:394
    Pin {
        number: 22,
        name: "MF_ISH_GPIO_4",
    }, // pinctrl-cherryview.c:395
    Pin {
        number: 23,
        name: "MF_ISH_GPIO_8",
    }, // pinctrl-cherryview.c:396
    Pin {
        number: 24,
        name: "MF_ISH_GPIO_2",
    }, // pinctrl-cherryview.c:397
    Pin {
        number: 25,
        name: "MF_ISH_GPIO_6",
    }, // pinctrl-cherryview.c:398
    Pin {
        number: 26,
        name: "MF_ISH_I2C1_SDA",
    }, // pinctrl-cherryview.c:399
];

pub const SOUTHEAST_PINS: [Pin; 55] = [
    Pin {
        number: 0,
        name: "MF_PLT_CLK0",
    }, // pinctrl-cherryview.c:420
    Pin {
        number: 1,
        name: "PWM1",
    }, // pinctrl-cherryview.c:421
    Pin {
        number: 2,
        name: "MF_PLT_CLK1",
    }, // pinctrl-cherryview.c:422
    Pin {
        number: 3,
        name: "MF_PLT_CLK4",
    }, // pinctrl-cherryview.c:423
    Pin {
        number: 4,
        name: "MF_PLT_CLK3",
    }, // pinctrl-cherryview.c:424
    Pin {
        number: 5,
        name: "PWM0",
    }, // pinctrl-cherryview.c:425
    Pin {
        number: 6,
        name: "MF_PLT_CLK5",
    }, // pinctrl-cherryview.c:426
    Pin {
        number: 7,
        name: "MF_PLT_CLK2",
    }, // pinctrl-cherryview.c:427
    Pin {
        number: 15,
        name: "SDMMC2_D3_CD_B",
    }, // pinctrl-cherryview.c:429
    Pin {
        number: 16,
        name: "SDMMC1_CLK",
    }, // pinctrl-cherryview.c:430
    Pin {
        number: 17,
        name: "SDMMC1_D0",
    }, // pinctrl-cherryview.c:431
    Pin {
        number: 18,
        name: "SDMMC2_D1",
    }, // pinctrl-cherryview.c:432
    Pin {
        number: 19,
        name: "SDMMC2_CLK",
    }, // pinctrl-cherryview.c:433
    Pin {
        number: 20,
        name: "SDMMC1_D2",
    }, // pinctrl-cherryview.c:434
    Pin {
        number: 21,
        name: "SDMMC2_D2",
    }, // pinctrl-cherryview.c:435
    Pin {
        number: 22,
        name: "SDMMC2_CMD",
    }, // pinctrl-cherryview.c:436
    Pin {
        number: 23,
        name: "SDMMC1_CMD",
    }, // pinctrl-cherryview.c:437
    Pin {
        number: 24,
        name: "SDMMC1_D1",
    }, // pinctrl-cherryview.c:438
    Pin {
        number: 25,
        name: "SDMMC2_D0",
    }, // pinctrl-cherryview.c:439
    Pin {
        number: 26,
        name: "SDMMC1_D3_CD_B",
    }, // pinctrl-cherryview.c:440
    Pin {
        number: 30,
        name: "SDMMC3_D1",
    }, // pinctrl-cherryview.c:442
    Pin {
        number: 31,
        name: "SDMMC3_CLK",
    }, // pinctrl-cherryview.c:443
    Pin {
        number: 32,
        name: "SDMMC3_D3",
    }, // pinctrl-cherryview.c:444
    Pin {
        number: 33,
        name: "SDMMC3_D2",
    }, // pinctrl-cherryview.c:445
    Pin {
        number: 34,
        name: "SDMMC3_CMD",
    }, // pinctrl-cherryview.c:446
    Pin {
        number: 35,
        name: "SDMMC3_D0",
    }, // pinctrl-cherryview.c:447
    Pin {
        number: 45,
        name: "MF_LPC_AD2",
    }, // pinctrl-cherryview.c:449
    Pin {
        number: 46,
        name: "LPC_CLKRUNB",
    }, // pinctrl-cherryview.c:450
    Pin {
        number: 47,
        name: "MF_LPC_AD0",
    }, // pinctrl-cherryview.c:451
    Pin {
        number: 48,
        name: "LPC_FRAMEB",
    }, // pinctrl-cherryview.c:452
    Pin {
        number: 49,
        name: "MF_LPC_CLKOUT1",
    }, // pinctrl-cherryview.c:453
    Pin {
        number: 50,
        name: "MF_LPC_AD3",
    }, // pinctrl-cherryview.c:454
    Pin {
        number: 51,
        name: "MF_LPC_CLKOUT0",
    }, // pinctrl-cherryview.c:455
    Pin {
        number: 52,
        name: "MF_LPC_AD1",
    }, // pinctrl-cherryview.c:456
    Pin {
        number: 60,
        name: "SPI1_MISO",
    }, // pinctrl-cherryview.c:458
    Pin {
        number: 61,
        name: "SPI1_CSO_B",
    }, // pinctrl-cherryview.c:459
    Pin {
        number: 62,
        name: "SPI1_CLK",
    }, // pinctrl-cherryview.c:460
    Pin {
        number: 63,
        name: "MMC1_D6",
    }, // pinctrl-cherryview.c:461
    Pin {
        number: 64,
        name: "SPI1_MOSI",
    }, // pinctrl-cherryview.c:462
    Pin {
        number: 65,
        name: "MMC1_D5",
    }, // pinctrl-cherryview.c:463
    Pin {
        number: 66,
        name: "SPI1_CS1_B",
    }, // pinctrl-cherryview.c:464
    Pin {
        number: 67,
        name: "MMC1_D4_SD_WE",
    }, // pinctrl-cherryview.c:465
    Pin {
        number: 68,
        name: "MMC1_D7",
    }, // pinctrl-cherryview.c:466
    Pin {
        number: 69,
        name: "MMC1_RCLK",
    }, // pinctrl-cherryview.c:467
    Pin {
        number: 75,
        name: "USB_OC1_B",
    }, // pinctrl-cherryview.c:469
    Pin {
        number: 76,
        name: "PMU_RESETBUTTON_B",
    }, // pinctrl-cherryview.c:470
    Pin {
        number: 77,
        name: "GPIO_ALERT",
    }, // pinctrl-cherryview.c:471
    Pin {
        number: 78,
        name: "SDMMC3_PWR_EN_B",
    }, // pinctrl-cherryview.c:472
    Pin {
        number: 79,
        name: "ILB_SERIRQ",
    }, // pinctrl-cherryview.c:473
    Pin {
        number: 80,
        name: "USB_OC0_B",
    }, // pinctrl-cherryview.c:474
    Pin {
        number: 81,
        name: "SDMMC3_CD_B",
    }, // pinctrl-cherryview.c:475
    Pin {
        number: 82,
        name: "SPKR",
    }, // pinctrl-cherryview.c:476
    Pin {
        number: 83,
        name: "SUSPWRDNACK",
    }, // pinctrl-cherryview.c:477
    Pin {
        number: 84,
        name: "SPARE_PIN",
    }, // pinctrl-cherryview.c:478
    Pin {
        number: 85,
        name: "SDMMC3_1P8_EN",
    }, // pinctrl-cherryview.c:479
];
