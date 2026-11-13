import 'package:flutter/material.dart';
import 'app_constants.dart';

/// AppTheme provides light and dark theme configurations with Material 3 support
/// Generated with Flutter Theme Generator - Clean, modular, and maintainable
///
/// Features:
/// ✅ Uses AppConstants for consistent design tokens
/// ✅ Modular structure with separate theme components
/// ✅ Material 3 compliant color schemes
/// ✅ Support for 6 contrast modes (light, dark, medium/high contrast variants)
/// ✅ Production-ready with proper type declarations
class AppTheme {
  AppTheme._(); // Private constructor to prevent instantiation

  // ═══════════════════════════════════════════════════════════════════════════════
  // 🎨 PUBLIC THEME GETTERS
  // ═══════════════════════════════════════════════════════════════════════════════

  /// Light theme configuration
  static ThemeData get lightTheme => theme(lightScheme());

  /// Dark theme configuration
  static ThemeData get darkTheme => theme(darkScheme());

  /// Light medium contrast theme
  static ThemeData get lightMediumContrast => theme(lightMediumContrastScheme());

  /// Light high contrast theme
  static ThemeData get lightHighContrast => theme(lightHighContrastScheme());

  /// Dark medium contrast theme
  static ThemeData get darkMediumContrast => theme(darkMediumContrastScheme());

  /// Dark high contrast theme
  static ThemeData get darkHighContrast => theme(darkHighContrastScheme());

  // ═══════════════════════════════════════════════════════════════════════════════
  // 🌈 COLOR SCHEMES - Material 3 compliant
  // ═══════════════════════════════════════════════════════════════════════════════

  /// Light color scheme
  ///
  /// "Lagoon + Moonlight": teal primary, steel-blue secondary (assistant
  /// surfaces), warm moon-gold tertiary accent, all on teal-tinted neutrals.
  static ColorScheme lightScheme() {
    return const ColorScheme(
      brightness: Brightness.light,
      primary: Color(0xFF00677F),
      surfaceTint: Color(0xFF00677F),
      onPrimary: Color(0xFFFFFFFF),
      primaryContainer: Color(0xFFB7EAFF),
      onPrimaryContainer: Color(0xFF001F28),
      secondary: Color(0xFF44617D),
      onSecondary: Color(0xFFFFFFFF),
      secondaryContainer: Color(0xFFCEE5FF),
      onSecondaryContainer: Color(0xFF001D32),
      tertiary: Color(0xFF745B13),
      onTertiary: Color(0xFFFFFFFF),
      tertiaryContainer: Color(0xFFFFDF96),
      onTertiaryContainer: Color(0xFF251A00),
      error: Color(0xFFBA1A1A),
      onError: Color(0xFFFFFFFF),
      errorContainer: Color(0xFFFFDAD6),
      onErrorContainer: Color(0xFF410002),
      surface: Color(0xFFF7FAFC),
      onSurface: Color(0xFF181C1E),
      onSurfaceVariant: Color(0xFF3F484D),
      outline: Color(0xFF6F787E),
      outlineVariant: Color(0xFFBFC8CE),
      shadow: Color(0xFF000000),
      scrim: Color(0xFF000000),
      inverseSurface: Color(0xFF2D3133),
      onInverseSurface: Color(0xFFEEF1F4),
      inversePrimary: Color(0xFF6ED3F6),
      primaryFixed: Color(0xFFB7EAFF),
      onPrimaryFixed: Color(0xFF001F28),
      primaryFixedDim: Color(0xFF6ED3F6),
      onPrimaryFixedVariant: Color(0xFF004E61),
      secondaryFixed: Color(0xFFCEE5FF),
      onSecondaryFixed: Color(0xFF001D32),
      secondaryFixedDim: Color(0xFFABCAEA),
      onSecondaryFixedVariant: Color(0xFF2B4964),
      tertiaryFixed: Color(0xFFFFDF96),
      onTertiaryFixed: Color(0xFF251A00),
      tertiaryFixedDim: Color(0xFFE5C372),
      onTertiaryFixedVariant: Color(0xFF5A4400),
      surfaceDim: Color(0xFFD7DADD),
      surfaceBright: Color(0xFFF7FAFC),
      surfaceContainerLowest: Color(0xFFFFFFFF),
      surfaceContainerLow: Color(0xFFF1F4F7),
      surfaceContainer: Color(0xFFEBEEF1),
      surfaceContainerHigh: Color(0xFFE6E8EB),
      surfaceContainerHighest: Color(0xFFE0E3E6),
    );
  }

  /// Dark color scheme
  ///
  /// Same "Lagoon + Moonlight" family: a *bright* teal primary (tone 80) so
  /// accents pop on the near-black teal-tinted neutrals.
  static ColorScheme darkScheme() {
    return const ColorScheme(
      brightness: Brightness.dark,
      primary: Color(0xFF6ED3F6),
      surfaceTint: Color(0xFF6ED3F6),
      onPrimary: Color(0xFF003543),
      primaryContainer: Color(0xFF004E61),
      onPrimaryContainer: Color(0xFFB7EAFF),
      secondary: Color(0xFFABCAEA),
      onSecondary: Color(0xFF12334C),
      secondaryContainer: Color(0xFF2B4964),
      onSecondaryContainer: Color(0xFFCEE5FF),
      tertiary: Color(0xFFE5C372),
      onTertiary: Color(0xFF3E2E00),
      tertiaryContainer: Color(0xFF5A4400),
      onTertiaryContainer: Color(0xFFFFDF96),
      error: Color(0xFFFFB4AB),
      onError: Color(0xFF690005),
      errorContainer: Color(0xFF93000A),
      onErrorContainer: Color(0xFFFFDAD6),
      surface: Color(0xFF101416),
      onSurface: Color(0xFFE0E3E6),
      onSurfaceVariant: Color(0xFFBFC8CE),
      outline: Color(0xFF899298),
      outlineVariant: Color(0xFF3F484D),
      shadow: Color(0xFF000000),
      scrim: Color(0xFF000000),
      inverseSurface: Color(0xFFE0E3E6),
      onInverseSurface: Color(0xFF2D3133),
      inversePrimary: Color(0xFF00677F),
      primaryFixed: Color(0xFFB7EAFF),
      onPrimaryFixed: Color(0xFF001F28),
      primaryFixedDim: Color(0xFF6ED3F6),
      onPrimaryFixedVariant: Color(0xFF004E61),
      secondaryFixed: Color(0xFFCEE5FF),
      onSecondaryFixed: Color(0xFF001D32),
      secondaryFixedDim: Color(0xFFABCAEA),
      onSecondaryFixedVariant: Color(0xFF2B4964),
      tertiaryFixed: Color(0xFFFFDF96),
      onTertiaryFixed: Color(0xFF251A00),
      tertiaryFixedDim: Color(0xFFE5C372),
      onTertiaryFixedVariant: Color(0xFF5A4400),
      surfaceDim: Color(0xFF101416),
      surfaceBright: Color(0xFF363A3C),
      surfaceContainerLowest: Color(0xFF0B0F11),
      surfaceContainerLow: Color(0xFF181C1E),
      surfaceContainer: Color(0xFF1C2022),
      surfaceContainerHigh: Color(0xFF272A2D),
      surfaceContainerHighest: Color(0xFF313538),
    );
  }

  /// Light medium contrast color scheme
  static ColorScheme lightMediumContrastScheme() {
    return lightScheme().copyWith(
      primary: Color(0xFF003C4B),
      surface: Color(0xFFF7FAFC),
    );
  }

  /// Light high contrast color scheme
  static ColorScheme lightHighContrastScheme() {
    return lightScheme().copyWith(
      primary: Color(0xFF00313E),
      surface: Color(0xFFF7FAFC),
      outline: const Color(0xff000000),
    );
  }

  /// Dark medium contrast color scheme
  static ColorScheme darkMediumContrastScheme() {
    return darkScheme().copyWith(
      primary: Color(0xFFA5E6FF),
      surface: Color(0xFF101416),
    );
  }

  /// Dark high contrast color scheme
  static ColorScheme darkHighContrastScheme() {
    return darkScheme().copyWith(
      primary: Color(0xFFDBF4FF),
      surface: Color(0xFF101416),
      outline: const Color(0xffffffff),
    );
  }

  // ═══════════════════════════════════════════════════════════════════════════════
  // 🎯 MAIN THEME BUILDER - Clean and modular structure
  // ═══════════════════════════════════════════════════════════════════════════════

  /// Main theme function that combines all theme components
  /// Uses clean, modular structure with proper AppConstants integration
  static ThemeData theme(ColorScheme colorScheme) => ThemeData(
    useMaterial3: true,
    brightness: colorScheme.brightness,
    colorScheme: colorScheme,
    textTheme: _textTheme,
    appBarTheme: colorScheme.brightness == Brightness.light ? _lightAppBarTheme : _darkAppBarTheme,
    elevatedButtonTheme: elevatedButtonTheme(colorScheme),
    filledButtonTheme: filledButtonTheme(colorScheme),
    textButtonTheme: textButtonTheme(colorScheme),
    outlinedButtonTheme: outlinedButtonTheme(colorScheme),
    iconButtonTheme: iconButtonTheme(colorScheme),
    inputDecorationTheme: _inputDecorationTheme,
    cardTheme: _cardTheme,
    chipTheme: _chipTheme,
    progressIndicatorTheme: _progressIndicatorTheme,
    dividerTheme: _dividerTheme,
    bottomNavigationBarTheme: _bottomNavigationBarTheme,
    tabBarTheme: _tabBarTheme,
    switchTheme: switchTheme(colorScheme),
    checkboxTheme: _checkboxTheme,
    radioTheme: _radioTheme,
    sliderTheme: _sliderTheme,
    scaffoldBackgroundColor: colorScheme.surface,
    canvasColor: colorScheme.surface,
  );

  // ═══════════════════════════════════════════════════════════════════════════════
  // 🎨 THEME COMPONENTS - All using AppConstants for consistency
  // ═══════════════════════════════════════════════════════════════════════════════


  /// Text theme using AppConstants for consistent font sizes
  static final TextTheme _textTheme = TextTheme(
    displayLarge: TextStyle(
      fontSize: AppConstants.fontSizeDisplayLarge,
      fontWeight: FontWeight.w400,
      letterSpacing: -0.25,
      height: 1.12,
    ),
    displayMedium: TextStyle(
      fontSize: AppConstants.fontSizeDisplayMedium,
      fontWeight: FontWeight.w400,
      letterSpacing: 0,
      height: 1.16,
    ),
    displaySmall: TextStyle(
      fontSize: AppConstants.fontSizeDisplaySmall,
      fontWeight: FontWeight.w400,
      letterSpacing: 0,
      height: 1.22,
    ),
    headlineLarge: TextStyle(
      fontSize: AppConstants.fontSizeHeadlineLarge,
      fontWeight: FontWeight.w400,
      letterSpacing: 0,
      height: 1.25,
    ),
    headlineMedium: TextStyle(
      fontSize: AppConstants.fontSizeHeadlineMedium,
      fontWeight: FontWeight.w400,
      letterSpacing: 0,
      height: 1.29,
    ),
    headlineSmall: TextStyle(
      fontSize: AppConstants.fontSizeHeadlineSmall,
      fontWeight: FontWeight.w400,
      letterSpacing: 0,
      height: 1.33,
    ),
    titleLarge: TextStyle(
      fontSize: AppConstants.fontSizeTitleLarge,
      fontWeight: FontWeight.w400,
      letterSpacing: 0,
      height: 1.27,
    ),
    titleMedium: TextStyle(
      fontSize: AppConstants.fontSizeTitleMedium,
      fontWeight: FontWeight.w500,
      letterSpacing: 0.15,
      height: 1.50,
    ),
    titleSmall: TextStyle(
      fontSize: AppConstants.fontSizeTitleSmall,
      fontWeight: FontWeight.w500,
      letterSpacing: 0.1,
      height: 1.43,
    ),
    labelLarge: TextStyle(
      fontSize: AppConstants.fontSizeLabelLarge,
      fontWeight: FontWeight.w500,
      letterSpacing: 0.1,
      height: 1.43,
    ),
    labelMedium: TextStyle(
      fontSize: AppConstants.fontSizeLabelMedium,
      fontWeight: FontWeight.w500,
      letterSpacing: 0.5,
      height: 1.33,
    ),
    labelSmall: TextStyle(
      fontSize: AppConstants.fontSizeLabelSmall,
      fontWeight: FontWeight.w500,
      letterSpacing: 0.5,
      height: 1.45,
    ),
    bodyLarge: TextStyle(
      fontSize: AppConstants.fontSizeBodyLarge,
      fontWeight: FontWeight.w400,
      letterSpacing: 0.15,
      height: 1.50,
    ),
    bodyMedium: TextStyle(
      fontSize: AppConstants.fontSizeBodyMedium,
      fontWeight: FontWeight.w400,
      letterSpacing: 0.25,
      height: 1.43,
    ),
    bodySmall: TextStyle(
      fontSize: AppConstants.fontSizeBodySmall,
      fontWeight: FontWeight.w400,
      letterSpacing: 0.4,
      height: 1.33,
    ),
  );


  /// Elevated button theme - M3 compliant with WidgetStateProperty
  static ElevatedButtonThemeData elevatedButtonTheme(ColorScheme colorScheme) => ElevatedButtonThemeData(
    style: ButtonStyle(
      elevation: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) return 0;
        if (states.contains(WidgetState.hovered)) return AppConstants.elevationLevel3;
        if (states.contains(WidgetState.pressed)) return AppConstants.elevationLevel1;
        return AppConstants.elevationLevel2;
      }),
      padding: WidgetStateProperty.all(
        EdgeInsets.symmetric(
          horizontal: AppConstants.spacingLG,
          vertical: AppConstants.spacingMD,
        ),
      ),
      shape: WidgetStateProperty.all(
        RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppConstants.radiusMD),
        ),
      ),
      backgroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.12);
        }
        return colorScheme.primary;
      }),
      foregroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.38);
        }
        return colorScheme.onPrimary;
      }),
      overlayColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.pressed)) {
          return colorScheme.onPrimary.withValues(alpha: 0.1);
        }
        if (states.contains(WidgetState.hovered)) {
          return colorScheme.onPrimary.withValues(alpha: 0.08);
        }
        if (states.contains(WidgetState.focused)) {
          return colorScheme.onPrimary.withValues(alpha: 0.1);
        }
        return null;
      }),
      shadowColor: WidgetStateProperty.all(colorScheme.shadow),
    ),
  );

  /// Filled button theme - M3 compliant with WidgetStateProperty
  static FilledButtonThemeData filledButtonTheme(ColorScheme colorScheme) => FilledButtonThemeData(
    style: ButtonStyle(
      padding: WidgetStateProperty.all(
        EdgeInsets.symmetric(
          horizontal: AppConstants.spacingLG,
          vertical: AppConstants.spacingMD,
        ),
      ),
      shape: WidgetStateProperty.all(
        RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppConstants.radiusMD),
        ),
      ),
      backgroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.12);
        }
        return colorScheme.primary;
      }),
      foregroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.38);
        }
        return colorScheme.onPrimary;
      }),
      overlayColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.pressed)) {
          return colorScheme.onPrimary.withValues(alpha: 0.1);
        }
        if (states.contains(WidgetState.hovered)) {
          return colorScheme.onPrimary.withValues(alpha: 0.08);
        }
        return null;
      }),
    ),
  );

  /// Text button theme - M3 compliant with WidgetStateProperty
  static TextButtonThemeData textButtonTheme(ColorScheme colorScheme) => TextButtonThemeData(
    style: ButtonStyle(
      padding: WidgetStateProperty.all(
        EdgeInsets.symmetric(
          horizontal: AppConstants.spacingLG,
          vertical: AppConstants.spacingMD,
        ),
      ),
      shape: WidgetStateProperty.all(
        RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppConstants.radiusMD),
        ),
      ),
      foregroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.38);
        }
        return colorScheme.primary;
      }),
      overlayColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.pressed)) {
          return colorScheme.primary.withValues(alpha: 0.1);
        }
        if (states.contains(WidgetState.hovered)) {
          return colorScheme.primary.withValues(alpha: 0.08);
        }
        if (states.contains(WidgetState.focused)) {
          return colorScheme.primary.withValues(alpha: 0.1);
        }
        return null;
      }),
    ),
  );

  /// Outlined button theme - M3 compliant with WidgetStateProperty
  static OutlinedButtonThemeData outlinedButtonTheme(ColorScheme colorScheme) => OutlinedButtonThemeData(
    style: ButtonStyle(
      padding: WidgetStateProperty.all(
        EdgeInsets.symmetric(
          horizontal: AppConstants.spacingLG,
          vertical: AppConstants.spacingMD,
        ),
      ),
      shape: WidgetStateProperty.all(
        RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppConstants.radiusMD),
        ),
      ),
      side: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return BorderSide(color: colorScheme.onSurface.withValues(alpha: 0.12));
        }
        if (states.contains(WidgetState.focused)) {
          return BorderSide(color: colorScheme.primary);
        }
        return BorderSide(color: colorScheme.outline);
      }),
      foregroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.38);
        }
        return colorScheme.primary;
      }),
      overlayColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.pressed)) {
          return colorScheme.primary.withValues(alpha: 0.1);
        }
        if (states.contains(WidgetState.hovered)) {
          return colorScheme.primary.withValues(alpha: 0.08);
        }
        return null;
      }),
    ),
  );

  /// Icon button theme - M3 compliant with WidgetStateProperty
  static IconButtonThemeData iconButtonTheme(ColorScheme colorScheme) => IconButtonThemeData(
    style: ButtonStyle(
      foregroundColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.disabled)) {
          return colorScheme.onSurface.withValues(alpha: 0.38);
        }
        return colorScheme.onSurfaceVariant;
      }),
      overlayColor: WidgetStateProperty.resolveWith((states) {
        if (states.contains(WidgetState.pressed)) {
          return colorScheme.onSurfaceVariant.withValues(alpha: 0.1);
        }
        if (states.contains(WidgetState.hovered)) {
          return colorScheme.onSurfaceVariant.withValues(alpha: 0.08);
        }
        return null;
      }),
    ),
  );


  /// Input decoration theme
  static final InputDecorationTheme _inputDecorationTheme = InputDecorationTheme(
    contentPadding: EdgeInsets.symmetric(
      horizontal: AppConstants.spacingMD,
      vertical: AppConstants.spacingMD,
    ),
    border: OutlineInputBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusMD),
    ),
    enabledBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusMD),
    ),
    focusedBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusMD),
    ),
    errorBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusMD),
    ),
  );


  /// App bar theme for light mode
  static final AppBarTheme _lightAppBarTheme = AppBarTheme(
    elevation: AppConstants.elevationLevel1,
    centerTitle: false,
    titleSpacing: AppConstants.spacingMD,
    scrolledUnderElevation: AppConstants.elevationLevel1,
  );

  /// App bar theme for dark mode
  static final AppBarTheme _darkAppBarTheme = AppBarTheme(
    elevation: AppConstants.elevationLevel1,
    centerTitle: false,
    titleSpacing: AppConstants.spacingMD,
    scrolledUnderElevation: AppConstants.elevationLevel1,
  );

  /// Card theme
  static final CardThemeData _cardTheme = CardThemeData(
    elevation: AppConstants.elevationLevel1,
    margin: EdgeInsets.all(AppConstants.spacingSM),
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusLG),
    ),
  );

  /// Chip theme
  static final ChipThemeData _chipTheme = ChipThemeData(
    padding: EdgeInsets.symmetric(
      horizontal: AppConstants.spacingMD,
      vertical: AppConstants.spacingSM,
    ),
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusFull),
    ),
  );

  /// Progress indicator theme
  static final ProgressIndicatorThemeData _progressIndicatorTheme = ProgressIndicatorThemeData();

  /// Divider theme
  static final DividerThemeData _dividerTheme = DividerThemeData(
    thickness: AppConstants.borderWidthThin,
    space: AppConstants.spacingMD,
  );

  /// Bottom navigation bar theme
  static final BottomNavigationBarThemeData _bottomNavigationBarTheme = BottomNavigationBarThemeData(
    type: BottomNavigationBarType.fixed,
  );

  /// Tab bar theme
  static final TabBarThemeData _tabBarTheme = TabBarThemeData(
    labelPadding: EdgeInsets.symmetric(
      horizontal: AppConstants.spacingMD,
      vertical: AppConstants.spacingSM,
    ),
  );

  /// Switch theme - uses colorScheme from theme() parameter
  static SwitchThemeData switchTheme(ColorScheme colorScheme) => SwitchThemeData(
    thumbColor: WidgetStateProperty.resolveWith((states) {
      if (states.contains(WidgetState.selected)) {
        return colorScheme.primary;
      }
      return null;
    }),
    trackColor: WidgetStateProperty.resolveWith((states) {
      if (states.contains(WidgetState.selected)) {
        return colorScheme.primaryContainer;
      }
      return null;
    }),
  );

  /// Checkbox theme
  static final CheckboxThemeData _checkboxTheme = CheckboxThemeData(
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(AppConstants.radiusXS),
    ),
  );

  /// Radio theme
  static final RadioThemeData _radioTheme = RadioThemeData();

  /// Slider theme
  static final SliderThemeData _sliderTheme = SliderThemeData();
}

/// Custom theme colors extension for additional brand colors.
///
/// These roles are not part of the Material [ColorScheme], so they are tuned
/// per brightness to stay legible on both light and dark surfaces.
extension CustomColors on ColorScheme {
  /// Success color for positive actions and states
  Color get success =>
      brightness == Brightness.dark ? const Color(0xFF7CD88F) : const Color(0xFF2E7D32);

  /// Warning color for caution states
  Color get warning =>
      brightness == Brightness.dark ? const Color(0xFFFFB74D) : const Color(0xFFB26A00);

  /// Info color for informational states
  Color get info =>
      brightness == Brightness.dark ? const Color(0xFF64B5F6) : const Color(0xFF1976D2);
}
