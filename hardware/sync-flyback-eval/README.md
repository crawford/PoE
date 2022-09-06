# Synchronous Flyback Converter Evaluation #

## Test Plan ##

### Core Converter ###

Populate:
  1. FB1-FB2
  2. R2-R3, R5-R14, R19-R21, R23-R25
  3. RV1-RV4
  4. L1-L2
  5. C1-C12, C15-C17, C22, C24, C26-C28
  6. D1-D2, D6
  7. Q1-Q3A
  8. U2
  9. T1-T2

Tune:
  1. Set RV1 to 22.1 kΩ.
  2. Set RV2 to 69.8 kΩ.
  3. Set RV3 to 69.8 kΩ.
  4. Set RV4 to 115 kΩ.

Test:
  1. Insufficient supply
    a. Apply 5-volt supply across VDDA (TP10) and VSSA (TP11).
    b. Measure voltage across R7 and verify that it is 0 V.

  2. 24-volt supply
    a. Apply 470-ohm resistance across +12V (TP8) and GNDD (TP13).
    b. Apply 24-volt supply across VDDA and VSSA.
    c. Measure voltage across R8 and verify that it is 255 μV.
    d. Measure voltage across R7 and verify that it is ~181 μV.

  3. 48-volt supply
    a. Connect 470-ohm resistance across +12V (TP8) and GNDD (TP13).
    b. Connect 24-volt supply across VDDA and VSSA.
    c. Measure voltage across R8 and verify that it is 255 μV.
    d. Measure voltage across R7 and verify that it is ~128 μV.


### Power over Ethernet ###

Populate:
  1. J1
  2. TR1
  3. C13-C14, C18-C21, C25
  4. R1, R15-R18
  5. D3-D5

Test:
  1. Passive PoE
    a. Connect to passive (24-volt) PoE PS.
    b. Measure voltage across VDDA and VSSA and verify that it is ~24 V.
    c. Verify that the yellow LED turns on.

  2. PoE+
    a. Connect 470-ohm resistance across +12V and GNDD.
    b. Connect to PoE+ PS.
    c. Measure voltage across VDDA and VSSA and verify that is is 37-57 V.
    d. Verify that the yellow LED turns on.
