import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np
import matplotlib

# Force headless mode
matplotlib.use("Agg")


def create_id_mapping(df, prefix, output_csv):
    """Creates short IDs for long names and exports the mapping to CSV."""
    unique_names = df["name"].unique()
    mapping_dict = {name: f"{prefix}{i}" for i, name in enumerate(unique_names)}

    mapping_df = pd.DataFrame(list(mapping_dict.items()), columns=["Original Name", "Short ID"])
    mapping_df.to_csv(output_csv, index=False)

    df["name"] = df["name"].map(mapping_dict)
    return df, mapping_dict


def stable_softmax(col):
    """Computes a numerically stable softmax for a pandas column."""
    exps = np.exp(col - np.max(col))
    return exps / np.sum(exps)


def run_comprehensive_analysis(strategy_file, scheduler_file, smoothing_span=15):
    # ==========================================
    # 1. Load and Clean Data
    # ==========================================
    df_strat = pd.read_csv(strategy_file).sort_values(by=["tick"]).ffill().fillna(0)
    df_sched = pd.read_csv(scheduler_file).sort_values(by=["tick"]).ffill().fillna(0)

    # Apply ID Mapping ONLY to Strategies
    df_strat, strat_map = create_id_mapping(df_strat, "S", "strategy_mapping.csv")

    # Add derived metrics
    df_strat["efficiency"] = df_strat["cov_increase"] / df_strat["attempts"].replace(0, 1)

    # Get last known states
    final_strat = df_strat.groupby("name").last().reset_index()

    # ==========================================
    # 2. Strategy Statistics (Time Series)
    # ==========================================
    metrics = ["probability", "rating", "cov_increase", "attempts", "accepted", "efficiency"]

    fig, axes = plt.subplots(2, 3, figsize=(22, 12), sharex=True)
    axes = axes.flatten()

    unique_strategies = df_strat["name"].unique()
    colors = sns.color_palette("husl", len(unique_strategies))

    for idx, metric in enumerate(metrics):
        ax = axes[idx]
        for s_idx, strategy in enumerate(unique_strategies):
            strat_data = df_strat[df_strat["name"] == strategy]

            raw_y = strat_data[metric]
            # Calculate Exponential Moving Average
            smoothed_y = raw_y.ewm(span=smoothing_span, adjust=False).mean()

            # Plot raw data as a faint background line
            ax.plot(strat_data["tick"], raw_y, color=colors[s_idx], linewidth=1, alpha=0.15)

            # Plot the smoothed trendline over top
            ax.plot(
                strat_data["tick"],
                smoothed_y,
                label=strategy if idx == 0 else "",  # Avoid duplicate legends
                color=colors[s_idx],
                linewidth=2.5,
                alpha=0.9,
            )

            if metric != "efficiency":
                ax.fill_between(strat_data["tick"], smoothed_y, 0, color=colors[s_idx], alpha=0.05)

        ax.set_title(metric.capitalize().replace("_", " "))
        ax.grid(True, linestyle="--", alpha=0.6)
        ax.tick_params(axis="x", rotation=45)

    fig.suptitle(
        f"Strategy Statistics Over Time (EMA Smoothed, Span={smoothing_span})", fontsize=20, y=1.02
    )

    handles, labels = axes[0].get_legend_handles_labels()
    fig.legend(
        handles,
        labels,
        loc="lower center",
        bbox_to_anchor=(0.5, -0.1),
        ncol=min(10, len(unique_strategies)),
        title="Strategy IDs",
    )

    plt.tight_layout()
    plt.savefig("strategy_development_expanded.png", dpi=300, bbox_inches="tight")
    plt.close()

    # ==========================================
    # 3. Strategy Behavior & Correlations
    # ==========================================
    fig, axes = plt.subplots(1, 3, figsize=(22, 6))
    fig.suptitle("Strategy Behavior & Efficiency Analysis", fontsize=18)

    sns.scatterplot(
        data=final_strat,
        x="attempts",
        y="cov_increase",
        size="probability",
        sizes=(50, 400),
        alpha=0.8,
        hue="name",
        legend=False,
        ax=axes[0],
    )
    for line in range(0, final_strat.shape[0]):
        axes[0].text(
            final_strat.attempts[line],
            final_strat.cov_increase[line],
            final_strat.name[line],
            horizontalalignment="left",
            size="small",
            color="black",
        )

    axes[0].set_title("Attempts vs. Coverage (Dot Size = Final Prob)")
    axes[0].set_xlabel("Total Attempts")
    axes[0].set_ylabel("Total Coverage Increase")

    global_cov = df_strat.groupby("tick")["cov_increase"].sum().reset_index()
    axes[1].plot(global_cov["tick"], global_cov["cov_increase"], color="purple", linewidth=3)
    axes[1].fill_between(
        global_cov["tick"], global_cov["cov_increase"], 0, color="purple", alpha=0.2
    )
    axes[1].set_title("Total Fuzzer Coverage Progress")
    axes[1].set_xlabel("Tick")
    axes[1].set_ylabel("Sum of All Strategies' Coverage")
    axes[1].grid(True, linestyle="--", alpha=0.6)

    strat_corr_cols = [
        "attempts",
        "accepted",
        "cov_increase",
        "rating",
        "probability",
        "efficiency",
    ]
    corr_matrix_strat = final_strat[strat_corr_cols].corr()
    sns.heatmap(
        corr_matrix_strat, annot=True, cmap="coolwarm", vmin=-1, vmax=1, ax=axes[2], fmt=".2f"
    )
    axes[2].set_title("Strategy Metric Correlations")

    plt.tight_layout()
    plt.savefig("strategy_behavior.png", dpi=300, bbox_inches="tight")
    plt.close()

    # ==========================================
    # 4. Corpus Scheduler Analysis (Heatmaps)
    # ==========================================
    fig, axes = plt.subplots(1, 2, figsize=(24, 12))

    heatmap_data = df_sched.pivot_table(index="name", columns="tick", values="rating")
    heatmap_data = heatmap_data.ffill(axis=1).fillna(0)
    first_appearance = df_sched.groupby("name")["tick"].min().sort_values()
    heatmap_data = heatmap_data.loc[first_appearance.index]

    # Turned yticklabels back on to show the actual corpus names
    sns.heatmap(
        heatmap_data, cmap="viridis", cbar_kws={"label": "Raw Rating"}, yticklabels=True, ax=axes[0]
    )
    axes[0].set_title("Raw Corpus Ratings Over Time", fontsize=16)
    axes[0].set_xlabel("Tick")
    axes[0].set_ylabel("Entries (Oldest at top, Newest at bottom)")
    # Rotate the y-axis labels if they are long hashes so they don't overlap
    axes[0].tick_params(axis="y", rotation=0, labelsize=8)

    softmax_heatmap = heatmap_data.apply(stable_softmax, axis=0)
    sns.heatmap(
        softmax_heatmap,
        cmap="magma",
        cbar_kws={"label": "Selection Probability (Softmax)"},
        yticklabels=True,
        ax=axes[1],
    )
    axes[1].set_title("Normalized Entry Dominance (Softmax)", fontsize=16)
    axes[1].set_xlabel("Tick")
    axes[1].set_ylabel("")
    axes[1].tick_params(axis="y", rotation=0, labelsize=8)

    plt.tight_layout()
    plt.savefig("scheduler_corpus_heatmaps.png", dpi=300, bbox_inches="tight")
    plt.close()


if __name__ == "__main__":
    run_comprehensive_analysis(
        "../../docker_out/perf_out/strategy_stats.csv",
        "../../docker_out/perf_out/scheduler_stats.csv",
    )
