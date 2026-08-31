// SPDX-License-Identifier: GPL-2.0-only
//! Frozen pin vectors from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::pins::{EAST_PINS, NORTH_PINS, SOUTHEAST_PINS, SOUTHWEST_PINS};

fn values(pins: &[pinctrl_cherryview_core::pins::Pin]) -> Vec<(u16, &'static str)> {
    pins.iter().map(|pin| (pin.number, pin.name)).collect()
}

#[test]
fn southwest_pin_count_numbers_and_names_match_linux() {
    // pinctrl-cherryview.c:104-165. This literal list is independent of the production table.
    assert_eq!(SOUTHWEST_PINS.len(), 56);
    assert_eq!(
        values(&SOUTHWEST_PINS),
        vec![
            (0, "FST_SPI_D2"),       // pinctrl-cherryview.c:104
            (1, "FST_SPI_D0"),       // pinctrl-cherryview.c:105
            (2, "FST_SPI_CLK"),      // pinctrl-cherryview.c:106
            (3, "FST_SPI_D3"),       // pinctrl-cherryview.c:107
            (4, "FST_SPI_CS1_B"),    // pinctrl-cherryview.c:108
            (5, "FST_SPI_D1"),       // pinctrl-cherryview.c:109
            (6, "FST_SPI_CS0_B"),    // pinctrl-cherryview.c:110
            (7, "FST_SPI_CS2_B"),    // pinctrl-cherryview.c:111
            (15, "UART1_RTS_B"),     // pinctrl-cherryview.c:113
            (16, "UART1_RXD"),       // pinctrl-cherryview.c:114
            (17, "UART2_RXD"),       // pinctrl-cherryview.c:115
            (18, "UART1_CTS_B"),     // pinctrl-cherryview.c:116
            (19, "UART2_RTS_B"),     // pinctrl-cherryview.c:117
            (20, "UART1_TXD"),       // pinctrl-cherryview.c:118
            (21, "UART2_TXD"),       // pinctrl-cherryview.c:119
            (22, "UART2_CTS_B"),     // pinctrl-cherryview.c:120
            (30, "MF_HDA_CLK"),      // pinctrl-cherryview.c:122
            (31, "MF_HDA_RSTB"),     // pinctrl-cherryview.c:123
            (32, "MF_HDA_SDIO"),     // pinctrl-cherryview.c:124
            (33, "MF_HDA_SDO"),      // pinctrl-cherryview.c:125
            (34, "MF_HDA_DOCKRSTB"), // pinctrl-cherryview.c:126
            (35, "MF_HDA_SYNC"),     // pinctrl-cherryview.c:127
            (36, "MF_HDA_SDI1"),     // pinctrl-cherryview.c:128
            (37, "MF_HDA_DOCKENB"),  // pinctrl-cherryview.c:129
            (45, "I2C5_SDA"),        // pinctrl-cherryview.c:131
            (46, "I2C4_SDA"),        // pinctrl-cherryview.c:132
            (47, "I2C6_SDA"),        // pinctrl-cherryview.c:133
            (48, "I2C5_SCL"),        // pinctrl-cherryview.c:134
            (49, "I2C_NFC_SDA"),     // pinctrl-cherryview.c:135
            (50, "I2C4_SCL"),        // pinctrl-cherryview.c:136
            (51, "I2C6_SCL"),        // pinctrl-cherryview.c:137
            (52, "I2C_NFC_SCL"),     // pinctrl-cherryview.c:138
            (60, "I2C1_SDA"),        // pinctrl-cherryview.c:140
            (61, "I2C0_SDA"),        // pinctrl-cherryview.c:141
            (62, "I2C2_SDA"),        // pinctrl-cherryview.c:142
            (63, "I2C1_SCL"),        // pinctrl-cherryview.c:143
            (64, "I2C3_SDA"),        // pinctrl-cherryview.c:144
            (65, "I2C0_SCL"),        // pinctrl-cherryview.c:145
            (66, "I2C2_SCL"),        // pinctrl-cherryview.c:146
            (67, "I2C3_SCL"),        // pinctrl-cherryview.c:147
            (75, "SATA_GP0"),        // pinctrl-cherryview.c:149
            (76, "SATA_GP1"),        // pinctrl-cherryview.c:150
            (77, "SATA_LEDN"),       // pinctrl-cherryview.c:151
            (78, "SATA_GP2"),        // pinctrl-cherryview.c:152
            (79, "MF_SMB_ALERTB"),   // pinctrl-cherryview.c:153
            (80, "SATA_GP3"),        // pinctrl-cherryview.c:154
            (81, "MF_SMB_CLK"),      // pinctrl-cherryview.c:155
            (82, "MF_SMB_DATA"),     // pinctrl-cherryview.c:156
            (90, "PCIE_CLKREQ0B"),   // pinctrl-cherryview.c:158
            (91, "PCIE_CLKREQ1B"),   // pinctrl-cherryview.c:159
            (92, "GP_SSP_2_CLK"),    // pinctrl-cherryview.c:160
            (93, "PCIE_CLKREQ2B"),   // pinctrl-cherryview.c:161
            (94, "GP_SSP_2_RXD"),    // pinctrl-cherryview.c:162
            (95, "PCIE_CLKREQ3B"),   // pinctrl-cherryview.c:163
            (96, "GP_SSP_2_FS"),     // pinctrl-cherryview.c:164
            (97, "GP_SSP_2_TXD"),    // pinctrl-cherryview.c:165
        ]
    );
}

#[test]
fn north_pin_count_numbers_and_names_match_linux() {
    // pinctrl-cherryview.c:285-347. This literal list is independent of the production table.
    assert_eq!(NORTH_PINS.len(), 59);
    assert_eq!(
        values(&NORTH_PINS),
        vec![
            (0, "GPIO_DFX_0"),       // pinctrl-cherryview.c:285
            (1, "GPIO_DFX_3"),       // pinctrl-cherryview.c:286
            (2, "GPIO_DFX_7"),       // pinctrl-cherryview.c:287
            (3, "GPIO_DFX_1"),       // pinctrl-cherryview.c:288
            (4, "GPIO_DFX_5"),       // pinctrl-cherryview.c:289
            (5, "GPIO_DFX_4"),       // pinctrl-cherryview.c:290
            (6, "GPIO_DFX_8"),       // pinctrl-cherryview.c:291
            (7, "GPIO_DFX_2"),       // pinctrl-cherryview.c:292
            (8, "GPIO_DFX_6"),       // pinctrl-cherryview.c:293
            (15, "GPIO_SUS0"),       // pinctrl-cherryview.c:295
            (16, "SEC_GPIO_SUS10"),  // pinctrl-cherryview.c:296
            (17, "GPIO_SUS3"),       // pinctrl-cherryview.c:297
            (18, "GPIO_SUS7"),       // pinctrl-cherryview.c:298
            (19, "GPIO_SUS1"),       // pinctrl-cherryview.c:299
            (20, "GPIO_SUS5"),       // pinctrl-cherryview.c:300
            (21, "SEC_GPIO_SUS11"),  // pinctrl-cherryview.c:301
            (22, "GPIO_SUS4"),       // pinctrl-cherryview.c:302
            (23, "SEC_GPIO_SUS8"),   // pinctrl-cherryview.c:303
            (24, "GPIO_SUS2"),       // pinctrl-cherryview.c:304
            (25, "GPIO_SUS6"),       // pinctrl-cherryview.c:305
            (26, "CX_PREQ_B"),       // pinctrl-cherryview.c:306
            (27, "SEC_GPIO_SUS9"),   // pinctrl-cherryview.c:307
            (30, "TRST_B"),          // pinctrl-cherryview.c:309
            (31, "TCK"),             // pinctrl-cherryview.c:310
            (32, "PROCHOT_B"),       // pinctrl-cherryview.c:311
            (33, "SVIDO_DATA"),      // pinctrl-cherryview.c:312
            (34, "TMS"),             // pinctrl-cherryview.c:313
            (35, "CX_PRDY_B_2"),     // pinctrl-cherryview.c:314
            (36, "TDO_2"),           // pinctrl-cherryview.c:315
            (37, "CX_PRDY_B"),       // pinctrl-cherryview.c:316
            (38, "SVIDO_ALERT_B"),   // pinctrl-cherryview.c:317
            (39, "TDO"),             // pinctrl-cherryview.c:318
            (40, "SVIDO_CLK"),       // pinctrl-cherryview.c:319
            (41, "TDI"),             // pinctrl-cherryview.c:320
            (45, "GP_CAMERASB_05"),  // pinctrl-cherryview.c:322
            (46, "GP_CAMERASB_02"),  // pinctrl-cherryview.c:323
            (47, "GP_CAMERASB_08"),  // pinctrl-cherryview.c:324
            (48, "GP_CAMERASB_00"),  // pinctrl-cherryview.c:325
            (49, "GP_CAMERASB_06"),  // pinctrl-cherryview.c:326
            (50, "GP_CAMERASB_10"),  // pinctrl-cherryview.c:327
            (51, "GP_CAMERASB_03"),  // pinctrl-cherryview.c:328
            (52, "GP_CAMERASB_09"),  // pinctrl-cherryview.c:329
            (53, "GP_CAMERASB_01"),  // pinctrl-cherryview.c:330
            (54, "GP_CAMERASB_07"),  // pinctrl-cherryview.c:331
            (55, "GP_CAMERASB_11"),  // pinctrl-cherryview.c:332
            (56, "GP_CAMERASB_04"),  // pinctrl-cherryview.c:333
            (60, "PANEL0_BKLTEN"),   // pinctrl-cherryview.c:335
            (61, "HV_DDI0_HPD"),     // pinctrl-cherryview.c:336
            (62, "HV_DDI2_DDC_SDA"), // pinctrl-cherryview.c:337
            (63, "PANEL1_BKLTCTL"),  // pinctrl-cherryview.c:338
            (64, "HV_DDI1_HPD"),     // pinctrl-cherryview.c:339
            (65, "PANEL0_BKLTCTL"),  // pinctrl-cherryview.c:340
            (66, "HV_DDI0_DDC_SDA"), // pinctrl-cherryview.c:341
            (67, "HV_DDI2_DDC_SCL"), // pinctrl-cherryview.c:342
            (68, "HV_DDI2_HPD"),     // pinctrl-cherryview.c:343
            (69, "PANEL1_VDDEN"),    // pinctrl-cherryview.c:344
            (70, "PANEL1_BKLTEN"),   // pinctrl-cherryview.c:345
            (71, "HV_DDI0_DDC_SCL"), // pinctrl-cherryview.c:346
            (72, "PANEL0_VDDEN"),    // pinctrl-cherryview.c:347
        ]
    );
}

#[test]
fn east_pin_count_numbers_and_names_match_linux() {
    // pinctrl-cherryview.c:375-399. This literal list is independent of the production table.
    assert_eq!(EAST_PINS.len(), 24);
    assert_eq!(
        values(&EAST_PINS),
        vec![
            (0, "PMU_SLP_S3_B"),     // pinctrl-cherryview.c:375
            (1, "PMU_BATLOW_B"),     // pinctrl-cherryview.c:376
            (2, "SUS_STAT_B"),       // pinctrl-cherryview.c:377
            (3, "PMU_SLP_S0IX_B"),   // pinctrl-cherryview.c:378
            (4, "PMU_AC_PRESENT"),   // pinctrl-cherryview.c:379
            (5, "PMU_PLTRST_B"),     // pinctrl-cherryview.c:380
            (6, "PMU_SUSCLK"),       // pinctrl-cherryview.c:381
            (7, "PMU_SLP_LAN_B"),    // pinctrl-cherryview.c:382
            (8, "PMU_PWRBTN_B"),     // pinctrl-cherryview.c:383
            (9, "PMU_SLP_S4_B"),     // pinctrl-cherryview.c:384
            (10, "PMU_WAKE_B"),      // pinctrl-cherryview.c:385
            (11, "PMU_WAKE_LAN_B"),  // pinctrl-cherryview.c:386
            (15, "MF_ISH_GPIO_3"),   // pinctrl-cherryview.c:388
            (16, "MF_ISH_GPIO_7"),   // pinctrl-cherryview.c:389
            (17, "MF_ISH_I2C1_SCL"), // pinctrl-cherryview.c:390
            (18, "MF_ISH_GPIO_1"),   // pinctrl-cherryview.c:391
            (19, "MF_ISH_GPIO_5"),   // pinctrl-cherryview.c:392
            (20, "MF_ISH_GPIO_9"),   // pinctrl-cherryview.c:393
            (21, "MF_ISH_GPIO_0"),   // pinctrl-cherryview.c:394
            (22, "MF_ISH_GPIO_4"),   // pinctrl-cherryview.c:395
            (23, "MF_ISH_GPIO_8"),   // pinctrl-cherryview.c:396
            (24, "MF_ISH_GPIO_2"),   // pinctrl-cherryview.c:397
            (25, "MF_ISH_GPIO_6"),   // pinctrl-cherryview.c:398
            (26, "MF_ISH_I2C1_SDA"), // pinctrl-cherryview.c:399
        ]
    );
}

#[test]
fn southeast_pin_count_numbers_and_names_match_linux() {
    // pinctrl-cherryview.c:420-479. This literal list is independent of the production table.
    assert_eq!(SOUTHEAST_PINS.len(), 55);
    assert_eq!(
        values(&SOUTHEAST_PINS),
        vec![
            (0, "MF_PLT_CLK0"),        // pinctrl-cherryview.c:420
            (1, "PWM1"),               // pinctrl-cherryview.c:421
            (2, "MF_PLT_CLK1"),        // pinctrl-cherryview.c:422
            (3, "MF_PLT_CLK4"),        // pinctrl-cherryview.c:423
            (4, "MF_PLT_CLK3"),        // pinctrl-cherryview.c:424
            (5, "PWM0"),               // pinctrl-cherryview.c:425
            (6, "MF_PLT_CLK5"),        // pinctrl-cherryview.c:426
            (7, "MF_PLT_CLK2"),        // pinctrl-cherryview.c:427
            (15, "SDMMC2_D3_CD_B"),    // pinctrl-cherryview.c:429
            (16, "SDMMC1_CLK"),        // pinctrl-cherryview.c:430
            (17, "SDMMC1_D0"),         // pinctrl-cherryview.c:431
            (18, "SDMMC2_D1"),         // pinctrl-cherryview.c:432
            (19, "SDMMC2_CLK"),        // pinctrl-cherryview.c:433
            (20, "SDMMC1_D2"),         // pinctrl-cherryview.c:434
            (21, "SDMMC2_D2"),         // pinctrl-cherryview.c:435
            (22, "SDMMC2_CMD"),        // pinctrl-cherryview.c:436
            (23, "SDMMC1_CMD"),        // pinctrl-cherryview.c:437
            (24, "SDMMC1_D1"),         // pinctrl-cherryview.c:438
            (25, "SDMMC2_D0"),         // pinctrl-cherryview.c:439
            (26, "SDMMC1_D3_CD_B"),    // pinctrl-cherryview.c:440
            (30, "SDMMC3_D1"),         // pinctrl-cherryview.c:442
            (31, "SDMMC3_CLK"),        // pinctrl-cherryview.c:443
            (32, "SDMMC3_D3"),         // pinctrl-cherryview.c:444
            (33, "SDMMC3_D2"),         // pinctrl-cherryview.c:445
            (34, "SDMMC3_CMD"),        // pinctrl-cherryview.c:446
            (35, "SDMMC3_D0"),         // pinctrl-cherryview.c:447
            (45, "MF_LPC_AD2"),        // pinctrl-cherryview.c:449
            (46, "LPC_CLKRUNB"),       // pinctrl-cherryview.c:450
            (47, "MF_LPC_AD0"),        // pinctrl-cherryview.c:451
            (48, "LPC_FRAMEB"),        // pinctrl-cherryview.c:452
            (49, "MF_LPC_CLKOUT1"),    // pinctrl-cherryview.c:453
            (50, "MF_LPC_AD3"),        // pinctrl-cherryview.c:454
            (51, "MF_LPC_CLKOUT0"),    // pinctrl-cherryview.c:455
            (52, "MF_LPC_AD1"),        // pinctrl-cherryview.c:456
            (60, "SPI1_MISO"),         // pinctrl-cherryview.c:458
            (61, "SPI1_CSO_B"),        // pinctrl-cherryview.c:459
            (62, "SPI1_CLK"),          // pinctrl-cherryview.c:460
            (63, "MMC1_D6"),           // pinctrl-cherryview.c:461
            (64, "SPI1_MOSI"),         // pinctrl-cherryview.c:462
            (65, "MMC1_D5"),           // pinctrl-cherryview.c:463
            (66, "SPI1_CS1_B"),        // pinctrl-cherryview.c:464
            (67, "MMC1_D4_SD_WE"),     // pinctrl-cherryview.c:465
            (68, "MMC1_D7"),           // pinctrl-cherryview.c:466
            (69, "MMC1_RCLK"),         // pinctrl-cherryview.c:467
            (75, "USB_OC1_B"),         // pinctrl-cherryview.c:469
            (76, "PMU_RESETBUTTON_B"), // pinctrl-cherryview.c:470
            (77, "GPIO_ALERT"),        // pinctrl-cherryview.c:471
            (78, "SDMMC3_PWR_EN_B"),   // pinctrl-cherryview.c:472
            (79, "ILB_SERIRQ"),        // pinctrl-cherryview.c:473
            (80, "USB_OC0_B"),         // pinctrl-cherryview.c:474
            (81, "SDMMC3_CD_B"),       // pinctrl-cherryview.c:475
            (82, "SPKR"),              // pinctrl-cherryview.c:476
            (83, "SUSPWRDNACK"),       // pinctrl-cherryview.c:477
            (84, "SPARE_PIN"),         // pinctrl-cherryview.c:478
            (85, "SDMMC3_1P8_EN"),     // pinctrl-cherryview.c:479
        ]
    );
}
