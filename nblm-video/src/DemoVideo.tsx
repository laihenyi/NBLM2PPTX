import {
  AbsoluteFill,
  Img,
  interpolate,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { locales, LocaleContent, LocaleKey } from "./locales";

export const DemoVideo = ({
  locale = "en",
}: {
  locale?: LocaleKey;
}) => {
  const frame = useCurrentFrame();
  const { fps, width, height } = useVideoConfig();
  const t = locales[locale];

  // Scene timing (in frames) - 40 seconds total
  const titleSceneDuration = 5 * fps; // 5 seconds
  const featuresSceneDuration = 8 * fps; // 8 seconds
  const demoSceneDuration = 10 * fps; // 10 seconds
  const workflowSceneDuration = 10 * fps; // 10 seconds
  const ctaSceneDuration = 7 * fps; // 7 seconds

  // Determine current scene
  const isTitleScene = frame < titleSceneDuration;
  const isFeaturesScene =
    frame >= titleSceneDuration && frame < titleSceneDuration + featuresSceneDuration;
  const isDemoScene =
    frame >= titleSceneDuration + featuresSceneDuration &&
    frame < titleSceneDuration + featuresSceneDuration + demoSceneDuration;
  const isWorkflowScene =
    frame >= titleSceneDuration + featuresSceneDuration + demoSceneDuration &&
    frame <
      titleSceneDuration + featuresSceneDuration + demoSceneDuration + workflowSceneDuration;
  const isCtaScene =
    frame >=
    titleSceneDuration + featuresSceneDuration + demoSceneDuration + workflowSceneDuration;

  // Animations
  const scaleIn = spring({
    frame,
    fps,
    config: { damping: 200 },
  });

  const fadeIn = interpolate(frame, [0, 1 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });

  const slideUp = interpolate(frame, [0, 1.5 * fps], [100, 0], {
    extrapolateRight: "clamp",
  });

  // Feature card animations
  const featureFrame = isFeaturesScene ? frame - titleSceneDuration : 0;
  const feature1Opacity = interpolate(featureFrame, [0, 0.5 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });
  const feature1X = interpolate(featureFrame, [0, 0.5 * fps], [-50, 0], {
    extrapolateRight: "clamp",
  });
  const feature2Opacity = interpolate(featureFrame, [0.5 * fps, 1 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });
  const feature2X = interpolate(featureFrame, [0.5 * fps, 1 * fps], [-50, 0], {
    extrapolateRight: "clamp",
  });
  const feature3Opacity = interpolate(featureFrame, [1 * fps, 1.5 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });
  const feature3X = interpolate(featureFrame, [1 * fps, 1.5 * fps], [-50, 0], {
    extrapolateRight: "clamp",
  });

  // Demo comparison animations
  const demoFrame = isDemoScene
    ? frame - titleSceneDuration - featuresSceneDuration
    : 0;

  const liteOpacity = interpolate(demoFrame, [0, 1 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });
  const liteScale = spring({
    frame: demoFrame,
    fps,
    config: { damping: 200 },
  });
  const standardOpacity = interpolate(demoFrame, [1 * fps, 2 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });
  const standardScale = spring({
    frame: demoFrame - 1 * fps,
    fps,
    config: { damping: 200 },
  });

  const v11Opacity = interpolate(demoFrame, [4 * fps, 5 * fps], [0, 1], {
    extrapolateRight: "clamp",
  });
  const v11Scale = spring({
    frame: demoFrame - 4 * fps,
    fps,
    config: { damping: 200 },
  });

  // Workflow animations
  const workflowFrame = isWorkflowScene
    ? frame -
      titleSceneDuration -
      featuresSceneDuration -
      demoSceneDuration
    : 0;

  // CTA animations
  const ctaFrame = isCtaScene
    ? frame -
      titleSceneDuration -
      featuresSceneDuration -
      demoSceneDuration -
      workflowSceneDuration
    : 0;
  const ctaScale = spring({
    frame: ctaFrame,
    fps,
    config: { damping: 15, stiffness: 80 },
  });

  const styles = {
    container: {
      backgroundColor: "#0f172a",
      fontFamily: "Arial, sans-serif",
    },
    title: {
      fontSize: 120,
      fontWeight: "bold",
      color: "#3b82f6",
      textAlign: "center",
      textShadow: "0 0 40px rgba(59, 130, 246, 0.5)",
    },
    subtitle: {
      fontSize: 36,
      color: "#94a3b8",
      textAlign: "center",
      marginTop: 20,
    },
    version: {
      fontSize: 24,
      color: "#64748b",
    },
    featureCard: {
      backgroundColor: "rgba(30, 41, 59, 0.8)",
      borderRadius: 20,
      padding: 40,
      margin: 20,
      border: "2px solid #3b82f6",
    },
    featureTitle: {
      fontSize: 32,
      fontWeight: "bold",
      color: "#3b82f6",
      marginBottom: 15,
    },
    featureDesc: {
      fontSize: 24,
      color: "#cbd5e1",
      lineHeight: 1.5,
    },
    demoCard: {
      backgroundColor: "rgba(30, 41, 59, 0.9)",
      borderRadius: 20,
      padding: 30,
      border: "2px solid #3b82f6",
    },
    demoImage: {
      width: "100%",
      height: 350,
      objectFit: "contain",
      borderRadius: 10,
      backgroundColor: "#1e293b",
    },
    demoLabel: {
      fontSize: 28,
      fontWeight: "bold",
      color: "#3b82f6",
      textAlign: "center",
      marginBottom: 15,
    },
    demoDesc: {
      fontSize: 20,
      color: "#94a3b8",
      textAlign: "center",
    },
    workflowStep: {
      display: "flex",
      alignItems: "center",
      margin: "30px 0",
    },
    stepNumber: {
      width: 80,
      height: 80,
      borderRadius: "50%",
      backgroundColor: "#3b82f6",
      color: "white",
      fontSize: 40,
      fontWeight: "bold",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      marginRight: 30,
    },
    stepText: {
      fontSize: 28,
      color: "#e2e8f0",
    },
    arrow: {
      fontSize: 40,
      color: "#3b82f6",
      margin: "0 30px",
    },
    ctaTitle: {
      fontSize: 60,
      fontWeight: "bold",
      color: "#3b82f6",
      textAlign: "center",
      marginBottom: 30,
    },
    ctaSubtitle: {
      fontSize: 28,
      color: "#94a3b8",
      textAlign: "center",
    },
    ctaLink: {
      fontSize: 32,
      color: "#3b82f6",
      fontWeight: "bold",
    },
  };

  return (
    <AbsoluteFill style={styles.container}>
      {isTitleScene && (
        <AbsoluteFill
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            alignItems: "center",
            opacity: fadeIn,
            transform: `scale(${scaleIn}) translateY(${slideUp}px)`,
          }}
        >
          <div style={styles.title}>{t.title}</div>
          <div style={styles.subtitle}>{t.subtitle}</div>
          <div style={{ ...styles.version, marginTop: 60 }}>
            {t.version}
          </div>
        </AbsoluteFill>
      )}

      {isFeaturesScene && (
        <AbsoluteFill
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            padding: 100,
          }}
        >
          <div
            style={{
              fontSize: 48,
              fontWeight: "bold",
              color: "#3b82f6",
              textAlign: "center",
              marginBottom: 50,
            }}
          >
            {t.keyFeatures}
          </div>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 30,
            }}
          >
            <div
              style={{
                ...styles.featureCard,
                opacity: feature1Opacity,
                transform: `translateX(${feature1X}px)`,
              }}
            >
              <div style={styles.featureTitle}>{t.features.title1}</div>
              <div style={styles.featureDesc}>
                {t.features.desc1.split("\n").map((line, i) => (
                  <span key={i}>
                    {line}
                    {i < t.features.desc1.split("\n").length - 1 && <br />}
                  </span>
                ))}
              </div>
            </div>
            <div
              style={{
                ...styles.featureCard,
                opacity: feature2Opacity,
                transform: `translateX(${feature2X}px)`,
              }}
            >
              <div style={styles.featureTitle}>{t.features.title2}</div>
              <div style={styles.featureDesc}>
                {t.features.desc2.split("\n").map((line, i) => (
                  <span key={i}>
                    {line}
                    {i < t.features.desc2.split("\n").length - 1 && <br />}
                  </span>
                ))}
              </div>
            </div>
            <div
              style={{
                ...styles.featureCard,
                opacity: feature3Opacity,
                transform: `translateX(${feature3X}px)`,
              }}
            >
              <div style={styles.featureTitle}>{t.features.title3}</div>
              <div style={styles.featureDesc}>
                {t.features.desc3.split("\n").map((line, i) => (
                  <span key={i}>
                    {line}
                    {i < t.features.desc3.split("\n").length - 1 && <br />}
                  </span>
                ))}
              </div>
            </div>
          </div>
        </AbsoluteFill>
      )}

      {isDemoScene && (
        <AbsoluteFill
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            padding: 80,
          }}
        >
          <div
            style={{
              fontSize: 48,
              fontWeight: "bold",
              color: "#3b82f6",
              textAlign: "center",
              marginBottom: 40,
            }}
          >
            {t.seeTheDifference}
          </div>

          <div
            style={{
              display: "flex",
              gap: 40,
              marginBottom: 40,
              opacity: demoFrame < 4 * fps ? 1 : 0,
              transform: `scale(${demoFrame < 4 * fps ? 1 : 0.8})`,
              transition: "all 0.5s ease",
            }}
          >
            <div
              style={{
                ...styles.demoCard,
                flex: 1,
                opacity: liteOpacity,
                transform: `scale(${liteScale})`,
              }}
            >
              <div style={styles.demoLabel}>{t.liteModel}</div>
              <Img
                src={staticFile("images/demo-v2.3-lite.jpg")}
                style={styles.demoImage as React.CSSProperties}
              />
              <div style={styles.demoDesc}>
                {t.liteModelDesc.split("\n").map((line, i) => (
                  <span key={i}>
                    {line}
                    {i < t.liteModelDesc.split("\n").length - 1 && <br />}
                  </span>
                ))}
              </div>
            </div>

            <div
              style={{
                ...styles.demoCard,
                flex: 1,
                opacity: standardOpacity,
                transform: `scale(${standardScale})`,
              }}
            >
              <div style={styles.demoLabel}>{t.standardModel}</div>
              <Img
                src={staticFile("images/demo-v2.3-standard.jpg")}
                style={styles.demoImage as React.CSSProperties}
              />
              <div style={styles.demoDesc}>
                {t.standardModelDesc.split("\n").map((line, i) => (
                  <span key={i}>
                    {line}
                    {i < t.standardModelDesc.split("\n").length - 1 && <br />}
                  </span>
                ))}
              </div>
            </div>
          </div>

          <div
            style={{
              opacity: v11Opacity,
              transform: `scale(${v11Scale})`,
            }}
          >
            <div
              style={{
                fontSize: 36,
                fontWeight: "bold",
                color: "#3b82f6",
                textAlign: "center",
                marginBottom: 30,
              }}
            >
              {t.hybridExtraction}
            </div>
            <div
              style={{
                display: "flex",
                gap: 40,
              }}
            >
              <div style={{ ...styles.demoCard, flex: 1 }}>
                <div style={styles.demoLabel}>{t.originalPdf}</div>
                <Img
                  src={staticFile("images/demo-v1.1-original.jpg")}
                  style={styles.demoImage as React.CSSProperties}
                />
              </div>

              <div style={{ ...styles.demoCard, flex: 1 }}>
                <div style={styles.demoLabel}>{t.pptxOutput}</div>
                <Img
                  src={staticFile("images/demo-v1.1-output.jpg")}
                  style={styles.demoImage as React.CSSProperties}
                />
                <div style={styles.demoDesc}>{t.pdfJsDesc}</div>
              </div>
            </div>
          </div>
        </AbsoluteFill>
      )}

      {isWorkflowScene && (
        <AbsoluteFill
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            alignItems: "center",
            padding: 100,
          }}
        >
          <div
            style={{
              fontSize: 48,
              fontWeight: "bold",
              color: "#3b82f6",
              textAlign: "center",
              marginBottom: 60,
            }}
          >
            {t.workflowTitle}
          </div>

          <div style={styles.workflowStep}>
            <div style={styles.stepNumber}>1</div>
            <div style={styles.stepText}>{t.steps.step1}</div>
            <div style={styles.arrow}>→</div>
          </div>

          <div style={styles.workflowStep}>
            <div style={styles.stepNumber}>2</div>
            <div style={styles.stepText}>{t.steps.step2}</div>
            <div style={styles.arrow}>→</div>
          </div>

          <div style={styles.workflowStep}>
            <div style={styles.stepNumber}>3</div>
            <div style={styles.stepText}>{t.steps.step3}</div>
            <div style={styles.arrow}>→</div>
          </div>

          <div style={styles.workflowStep}>
            <div style={styles.stepNumber}>4</div>
            <div style={styles.stepText}>{t.steps.step4}</div>
          </div>

          <div
            style={{
              marginTop: 60,
              fontSize: 24,
              color: "#64748b",
              textAlign: "center",
            }}
          >
            {t.workflowNote}
          </div>
        </AbsoluteFill>
      )}

      {isCtaScene && (
        <AbsoluteFill
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            alignItems: "center",
          }}
        >
          <div
            style={{
              transform: `scale(${ctaScale})`,
            }}
          >
            <div style={styles.ctaTitle}>{t.ctaTitle}</div>
            <div style={styles.ctaSubtitle}>{t.ctaSubtitle}</div>
            <div style={{ ...styles.ctaLink, marginTop: 50 }}>
              github.com/laihenyi/NBLM2PPTX
            </div>
          </div>
        </AbsoluteFill>
      )}
    </AbsoluteFill>
  );
};
